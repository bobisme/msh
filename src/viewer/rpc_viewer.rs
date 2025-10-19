#[cfg(feature = "remote")]
use crossbeam::channel::{self, Receiver, Sender};
#[cfg(feature = "remote")]
use kiss3d::event::{Action, Key};
#[cfg(feature = "remote")]
use kiss3d::light::Light;
#[cfg(feature = "remote")]
use kiss3d::nalgebra as na;
#[cfg(feature = "remote")]
use kiss3d::text::Font;
#[cfg(feature = "remote")]
use kiss3d::window::Window;
#[cfg(feature = "remote")]
use std::cell::RefCell;
#[cfg(feature = "remote")]
use std::path::PathBuf;
#[cfg(feature = "remote")]
use std::rc::Rc;
#[cfg(feature = "remote")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "remote")]
use super::state::{MeshStats, ViewerCommand, ViewerState};
#[cfg(feature = "remote")]
use crate::mesh::loader::load_mesh;
#[cfg(feature = "remote")]
use crate::rpc::spawn_rpc_server;

#[cfg(feature = "remote")]
pub async fn view_mesh_with_rpc(
    input: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);

    // Load initial mesh
    let mesh = load_mesh(input, mesh_name)?;

    // Calculate mesh statistics
    let vertex_count = mesh.count_vertices();
    let face_count = mesh.count_faces();
    let edge_count = mesh.unique_edges().count();
    let boundary_rings = mesh.boundary_rings();
    let is_manifold = boundary_rings.is_empty();

    let stats = MeshStats {
        vertex_count,
        edge_count,
        face_count,
        is_manifold,
        hole_count: boundary_rings.len(),
    };

    // Calculate bounding box
    let mut min = [f32::INFINITY, f32::INFINITY, f32::INFINITY];
    let mut max = [f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];

    for vertex_id in mesh.vertices() {
        let pos = mesh.vertex_position(vertex_id);
        min[0] = min[0].min(pos.x);
        min[1] = min[1].min(pos.y);
        min[2] = min[2].min(pos.z);
        max[0] = max[0].max(pos.x);
        max[1] = max[1].max(pos.y);
        max[2] = max[2].max(pos.z);
    }

    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let max_dimension = size[0].max(size[1]).max(size[2]);

    // Create shared state
    let state = Arc::new(Mutex::new(ViewerState::for_mesh(max_dimension, stats)));

    // Create command channel
    let (command_tx, command_rx): (Sender<ViewerCommand>, Receiver<ViewerCommand>) =
        channel::unbounded();

    // Spawn RPC server in background thread
    let state_clone = Arc::clone(&state);
    let _rpc_handle = spawn_rpc_server(state_clone, command_tx, 9001);

    // Run viewer with command processing
    run_viewer_with_commands(mesh, state, command_rx, max_dimension).await?;

    Ok(())
}

#[cfg(feature = "remote")]
async fn run_viewer_with_commands(
    initial_mesh: baby_shark::mesh::corner_table::CornerTableF,
    state: Arc<Mutex<ViewerState>>,
    command_rx: Receiver<ViewerCommand>,
    initial_max_dimension: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    use baby_shark::io::write_to_file;

    // Write to temporary OBJ file
    let temp_obj = std::env::temp_dir().join("msh_temp_view.obj");
    write_to_file(&initial_mesh, &temp_obj)
        .map_err(|e| format!("Failed to write temp mesh: {:?}", e))?;

    // Calculate center for initial mesh
    let mut min = [f32::INFINITY, f32::INFINITY, f32::INFINITY];
    let mut max = [f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];

    for vertex_id in initial_mesh.vertices() {
        let pos = initial_mesh.vertex_position(vertex_id);
        min[0] = min[0].min(pos.x);
        min[1] = min[1].min(pos.y);
        min[2] = min[2].min(pos.z);
        max[0] = max[0].max(pos.x);
        max[1] = max[1].max(pos.y);
        max[2] = max[2].max(pos.z);
    }

    let center = [
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
        (min[2] + max[2]) / 2.0,
    ];

    // Extract geometry
    let mut vertices: Vec<na::Point3<f32>> = Vec::new();
    let mut indices: Vec<na::Point3<u32>> = Vec::new();
    let mut vertex_idx = 0u32;

    for face_id in initial_mesh.faces() {
        let triangle = initial_mesh.face_positions(face_id);
        let v0 = triangle.p1();
        let v1 = triangle.p2();
        let v2 = triangle.p3();

        vertices.push(na::Point3::new(
            v0.x - center[0],
            v0.y - center[1],
            v0.z - center[2],
        ));
        vertices.push(na::Point3::new(
            v1.x - center[0],
            v1.y - center[1],
            v1.z - center[2],
        ));
        vertices.push(na::Point3::new(
            v2.x - center[0],
            v2.y - center[1],
            v2.z - center[2],
        ));

        indices.push(na::Point3::new(vertex_idx, vertex_idx + 1, vertex_idx + 2));
        vertex_idx += 3;
    }

    // Create reversed mesh for backfaces
    let mut reversed_indices: Vec<na::Point3<u32>> = Vec::new();
    for tri in &indices {
        reversed_indices.push(na::Point3::new(tri.x, tri.z, tri.y));
    }

    println!("Creating viewer window with RPC server enabled...");
    let mut window = Window::new("Mesh Viewer - msh (RPC Enabled)");
    window.set_light(Light::StickToCamera);

    // Main mesh
    let mesh_rc = Rc::new(RefCell::new(kiss3d::resource::GpuMesh::new(
        vertices.clone(),
        indices,
        None,
        None,
        false,
    )));

    let mut mesh_obj = window.add_mesh(mesh_rc, na::Vector3::new(1.0, 1.0, 1.0));
    mesh_obj.set_color(0.8, 0.8, 0.8);
    mesh_obj.enable_backface_culling(true);
    mesh_obj.set_lines_width(1.0);
    mesh_obj.set_lines_color(Some(na::Point3::new(0.0, 0.0, 0.0)));
    mesh_obj.set_surface_rendering_activation(true);

    // Backface mesh
    let backface_mesh_rc = Rc::new(RefCell::new(kiss3d::resource::GpuMesh::new(
        vertices,
        reversed_indices,
        None,
        None,
        false,
    )));

    let mut backface_obj = window.add_mesh(backface_mesh_rc, na::Vector3::new(1.0, 1.0, 1.0));
    backface_obj.set_color(1.0, 0.0, 0.0);
    backface_obj.enable_backface_culling(true);
    backface_obj.set_visible(false);

    // Camera setup
    let camera_distance = initial_max_dimension * 2.5;
    let eye = na::Point3::new(
        camera_distance * 0.5,
        camera_distance * 0.3,
        camera_distance,
    );
    let at = na::Point3::new(0.0, 0.0, 0.0);
    let mut arc_ball = kiss3d::camera::ArcBall::new(eye, at);

    let font = Arc::new(Font::default());

    // Initialize RenderDoc if available
    #[cfg(feature = "renderdoc")]
    let mut renderdoc = crate::viewer::renderdoc_helper::RenderDocCapture::new();

    println!("Viewer ready. RPC server listening on http://127.0.0.1:9001");
    println!("  W: Toggle wireframe");
    println!("  B: Toggle backfaces");
    println!("  U: Toggle UI");
    println!("  Q/ESC: Exit");

    while window.render_with_camera(&mut arc_ball).await {
        // Process RPC commands (non-blocking)
        while let Ok(cmd) = command_rx.try_recv() {
            match cmd {
                ViewerCommand::SetRotation { x, y, z } => {
                    let mut st = state.lock().unwrap();
                    st.model_rotation = na::Vector3::new(x, y, z);
                    let rot = na::UnitQuaternion::from_euler_angles(x, y, z);
                    mesh_obj.set_local_rotation(rot);
                    backface_obj.set_local_rotation(rot);
                    println!("Set rotation to ({}, {}, {})", x, y, z);
                }
                ViewerCommand::RotateAroundAxis { axis, angle } => {
                    let mut st = state.lock().unwrap();
                    st.apply_rotation(axis, angle);
                    let rot = na::UnitQuaternion::from_euler_angles(
                        st.model_rotation.x,
                        st.model_rotation.y,
                        st.model_rotation.z,
                    );
                    mesh_obj.set_local_rotation(rot);
                    backface_obj.set_local_rotation(rot);
                    println!("Rotated around axis {:?} by {} rad", axis, angle);
                }
                ViewerCommand::SetCameraPosition { position } => {
                    arc_ball.set_at(arc_ball.at()); // Keep target same
                    // Note: kiss3d's ArcBall doesn't expose set_eye directly,
                    // so we rebuild it
                    let target = arc_ball.at();
                    arc_ball = kiss3d::camera::ArcBall::new(position, target);
                    let mut st = state.lock().unwrap();
                    st.camera_position = position;
                    println!("Set camera position to {:?}", position);
                }
                ViewerCommand::SetCameraTarget { target } => {
                    let eye = na::Point3::new(
                        arc_ball.at().x + (arc_ball.at().x - target.x),
                        arc_ball.at().y + (arc_ball.at().y - target.y),
                        arc_ball.at().z + (arc_ball.at().z - target.z),
                    );
                    arc_ball = kiss3d::camera::ArcBall::new(eye, target);
                    let mut st = state.lock().unwrap();
                    st.camera_target = target;
                    println!("Set camera target to {:?}", target);
                }
                ViewerCommand::ToggleWireframe(enabled) => {
                    if enabled {
                        mesh_obj.set_lines_width(1.0);
                        mesh_obj.set_lines_color(Some(na::Point3::new(0.0, 0.0, 0.0)));
                    } else {
                        mesh_obj.set_lines_width(0.0);
                    }
                    let mut st = state.lock().unwrap();
                    st.show_wireframe = enabled;
                    println!("Wireframe: {}", if enabled { "ON" } else { "OFF" });
                }
                ViewerCommand::ToggleBackfaces(enabled) => {
                    backface_obj.set_visible(enabled);
                    let mut st = state.lock().unwrap();
                    st.show_backfaces = enabled;
                    println!("Backfaces: {}", if enabled { "ON" } else { "OFF" });
                }
                ViewerCommand::ToggleUI(enabled) => {
                    let mut st = state.lock().unwrap();
                    st.show_ui = enabled;
                    println!("UI: {}", if enabled { "ON" } else { "OFF" });
                }
                ViewerCommand::LoadModel { path, mesh_name } => {
                    println!("Load model command received, but not yet fully implemented");
                    println!("  Path: {:?}, Mesh: {:?}", path, mesh_name);
                    // TODO: Implement dynamic mesh loading
                }
                #[cfg(feature = "renderdoc")]
                ViewerCommand::CaptureFrame { path } => {
                    renderdoc.trigger_capture(path.as_deref());
                }
                ViewerCommand::Screenshot { path } => {
                    // Create parent directories if they don't exist
                    if let Some(parent) = std::path::Path::new(&path).parent() {
                        if !parent.as_os_str().is_empty() {
                            if let Err(e) = std::fs::create_dir_all(parent) {
                                eprintln!(
                                    "❌ Failed to create directory {}: {}",
                                    parent.display(),
                                    e
                                );
                                continue;
                            }
                        }
                    }

                    let img = window.snap_image();
                    match img.save(&path) {
                        Ok(_) => println!("📸 Screenshot saved to: {}", path),
                        Err(e) => eprintln!("❌ Failed to save screenshot: {}", e),
                    }
                }
            }
        }

        // Draw UI if enabled
        let st = state.lock().unwrap();
        if st.show_ui {
            draw_ui_overlay(&mut window, &font, &st);
        }
        drop(st);

        // Handle keyboard input
        for event in window.events().iter() {
            match event.value {
                kiss3d::event::WindowEvent::Key(Key::W, Action::Press, _) => {
                    let mut st = state.lock().unwrap();
                    st.show_wireframe = !st.show_wireframe;
                    if st.show_wireframe {
                        mesh_obj.set_lines_width(1.0);
                        mesh_obj.set_lines_color(Some(na::Point3::new(0.0, 0.0, 0.0)));
                    } else {
                        mesh_obj.set_lines_width(0.0);
                    }
                    println!(
                        "Wireframe: {}",
                        if st.show_wireframe { "ON" } else { "OFF" }
                    );
                }
                kiss3d::event::WindowEvent::Key(Key::B, Action::Press, _) => {
                    let mut st = state.lock().unwrap();
                    st.show_backfaces = !st.show_backfaces;
                    backface_obj.set_visible(st.show_backfaces);
                    println!(
                        "Backfaces: {}",
                        if st.show_backfaces { "ON" } else { "OFF" }
                    );
                }
                kiss3d::event::WindowEvent::Key(Key::U, Action::Press, _) => {
                    let mut st = state.lock().unwrap();
                    st.show_ui = !st.show_ui;
                    println!("UI: {}", if st.show_ui { "ON" } else { "OFF" });
                }
                kiss3d::event::WindowEvent::Key(Key::Q, Action::Press, _)
                | kiss3d::event::WindowEvent::Key(Key::Escape, Action::Press, _) => {
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    Ok(())
}

#[cfg(feature = "remote")]
fn draw_ui_overlay(window: &mut Window, font: &Arc<Font>, state: &ViewerState) {
    let x_offset = 11.0;
    let y_offset = 15.0;
    let line_height = 18.0;
    let header_size = 26.0;
    let text_size = 18.0;
    let header_padding = 8.0;

    let header_color = na::Point3::new(0.8, 0.8, 0.8);
    let text_color = na::Point3::new(0.9, 0.9, 0.9);

    window.draw_text(
        "Controls",
        &na::Point2::new(x_offset - 1.0, y_offset),
        header_size,
        font,
        &header_color,
    );
    let mut current_y = y_offset + line_height + header_padding;

    window.draw_text(
        "W: Toggle Wireframe",
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    window.draw_text(
        "B: Toggle Backfaces",
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    window.draw_text(
        "U: Toggle UI",
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    window.draw_text(
        "Q/ESC: Exit",
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );

    current_y += line_height * 2.0;
    window.draw_text(
        "Mesh Info",
        &na::Point2::new(x_offset - 1.0, current_y),
        header_size,
        font,
        &header_color,
    );
    current_y += line_height + header_padding;

    window.draw_text(
        &format!("Vertices: {}", state.stats.vertex_count),
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    window.draw_text(
        &format!("Edges: {}", state.stats.edge_count),
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    window.draw_text(
        &format!("Faces: {}", state.stats.face_count),
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    let manifold_text = if state.stats.is_manifold {
        "Manifold: Yes".to_string()
    } else {
        format!("Manifold: No ({} holes)", state.stats.hole_count)
    };
    let manifold_color = if state.stats.is_manifold {
        na::Point3::new(0.4, 1.0, 0.4)
    } else {
        na::Point3::new(1.0, 0.4, 0.4)
    };
    window.draw_text(
        &manifold_text,
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &manifold_color,
    );

    // RPC Status
    current_y += line_height * 2.0;
    window.draw_text(
        "RPC Server",
        &na::Point2::new(x_offset - 1.0, current_y),
        header_size,
        font,
        &header_color,
    );
    current_y += line_height + header_padding;

    let rpc_color = na::Point3::new(0.4, 1.0, 0.4);
    window.draw_text(
        "Active: 127.0.0.1:9001",
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &rpc_color,
    );
}
