use kiss3d::event::{Action, Key};
use kiss3d::light::Light;
use kiss3d::nalgebra as na;
use kiss3d::text::Font;
use kiss3d::window::Window;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use crate::mesh::loader::load_mesh;

pub fn view_mesh(input: &PathBuf, mesh_name: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);

    // Load mesh through baby_shark, export to temp OBJ, then load with kiss3d's OBJ loader
    let mesh = load_mesh(input, mesh_name)?;

    // Write to temporary OBJ file
    let temp_obj = std::env::temp_dir().join("msh_temp_view.obj");
    println!("Converting to OBJ format...");

    use baby_shark::io::write_to_file;
    write_to_file(&mesh, &temp_obj).map_err(|e| format!("Failed to write temp mesh: {:?}", e))?;

    // Calculate bounding box to center and scale the mesh
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

    let center = [
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
        (min[2] + max[2]) / 2.0,
    ];

    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];

    let max_dimension = size[0].max(size[1]).max(size[2]);

    println!(
        "Mesh bounds: ({:.3}, {:.3}, {:.3}) to ({:.3}, {:.3}, {:.3})",
        min[0], min[1], min[2], max[0], max[1], max[2]
    );
    println!(
        "Mesh center: ({:.3}, {:.3}, {:.3})",
        center[0], center[1], center[2]
    );
    println!("Mesh size: {:.3}", max_dimension);

    // Extract as triangle soup (no vertex sharing) to avoid any indexing issues
    let mut vertices: Vec<na::Point3<f32>> = Vec::new();
    let mut indices: Vec<na::Point3<u32>> = Vec::new();

    let mut vertex_idx = 0u32;

    for face_id in mesh.faces() {
        let triangle = mesh.face_positions(face_id);

        // Get the three vertices of the triangle
        let v0 = triangle.p1();
        let v1 = triangle.p2();
        let v3 = triangle.p3();

        // Add vertices directly (centered)
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
            v3.x - center[0],
            v3.y - center[1],
            v3.z - center[2],
        ));

        // Create triangle with sequential indices
        indices.push(na::Point3::new(vertex_idx, vertex_idx + 1, vertex_idx + 2));
        vertex_idx += 3;
    }

    println!(
        "Extracted {} vertices ({} triangles) as triangle soup",
        vertices.len(),
        indices.len()
    );

    // Calculate mesh statistics for overlay
    let vertex_count = mesh.count_vertices();
    let face_count = mesh.count_faces();
    let edge_count = mesh.unique_edges().count();
    let boundary_rings = mesh.boundary_rings();
    let is_manifold = boundary_rings.is_empty();

    // Create reversed mesh for backface visualization (flip winding)
    let mut reversed_indices: Vec<na::Point3<u32>> = Vec::new();
    for tri in &indices {
        // Reverse winding order: (v0, v1, v2) -> (v0, v2, v1)
        reversed_indices.push(na::Point3::new(tri.x, tri.z, tri.y));
    }

    println!("Creating viewer window...");
    let mut window = Window::new("Mesh Viewer - msh");
    window.set_light(Light::StickToCamera);

    // Main mesh (front faces)
    let mesh_rc = Rc::new(RefCell::new(kiss3d::resource::Mesh::new(
        vertices.clone(),
        indices,
        None,
        None,
        false,
    )));

    let mut mesh_obj = window.add_mesh(mesh_rc, na::Vector3::new(1.0, 1.0, 1.0));

    mesh_obj.set_color(0.8, 0.8, 0.8);
    mesh_obj.enable_backface_culling(true); // Always cull backfaces on main mesh

    // Enable wireframe overlay by default (surfaces + black edges)
    mesh_obj.set_lines_width(1.0);
    mesh_obj.set_lines_color(Some(na::Point3::new(0.0, 0.0, 0.0)));
    mesh_obj.set_surface_rendering_activation(true);

    // Backface mesh (reversed, red) - hidden by default
    let backface_mesh_rc = Rc::new(RefCell::new(kiss3d::resource::Mesh::new(
        vertices,
        reversed_indices,
        None,
        None,
        false,
    )));

    let mut backface_obj = window.add_mesh(backface_mesh_rc, na::Vector3::new(1.0, 1.0, 1.0));

    backface_obj.set_color(1.0, 0.0, 0.0); // Red
    backface_obj.enable_backface_culling(true); // Cull backfaces on reversed mesh too
    backface_obj.set_visible(false); // Hidden by default

    // Set camera to look at the centered mesh from a good distance
    let camera_distance = max_dimension * 2.5;
    let eye = na::Point3::new(
        camera_distance * 0.5,
        camera_distance * 0.3,
        camera_distance,
    );
    let at = na::Point3::new(0.0, 0.0, 0.0);
    let mut arc_ball = kiss3d::camera::ArcBall::new(eye, at);

    // State for interactive controls
    let mut show_wireframe = true; // On by default
    let mut show_backfaces = false;
    let mut show_ui = true; // On by default

    println!("Viewing mesh...");
    println!("  Mouse: Rotate (drag), Zoom (scroll), Pan (right-drag)");
    println!("  W: Toggle wireframe overlay");
    println!("  B: Toggle backface visualization (red)");
    println!("  U: Toggle UI overlay");
    println!("  Q/ESC: Exit");
    println!("Wireframe: ON (default)");

    // Load font for text rendering (use built-in font)
    let font = Arc::new(Font::default());

    while window.render_with_camera(&mut arc_ball) {
        // Draw UI overlay only if enabled
        if show_ui {
            draw_ui_overlay(
                &mut window,
                &font,
                vertex_count,
                edge_count,
                face_count,
                is_manifold,
                boundary_rings.len(),
            );
        }

        // Handle keyboard input
        for event in window.events().iter() {
            match event.value {
                kiss3d::event::WindowEvent::Key(Key::W, Action::Press, _) => {
                    show_wireframe = !show_wireframe;
                    if show_wireframe {
                        mesh_obj.set_lines_width(1.0);
                        mesh_obj.set_lines_color(Some(na::Point3::new(0.0, 0.0, 0.0)));
                    } else {
                        mesh_obj.set_lines_width(0.0);
                    }
                    println!("Wireframe: {}", if show_wireframe { "ON" } else { "OFF" });
                }
                kiss3d::event::WindowEvent::Key(Key::B, Action::Press, _) => {
                    show_backfaces = !show_backfaces;
                    backface_obj.set_visible(show_backfaces);
                    println!(
                        "Backface visualization: {}",
                        if show_backfaces { "ON (red)" } else { "OFF" }
                    );
                }
                kiss3d::event::WindowEvent::Key(Key::U, Action::Press, _) => {
                    show_ui = !show_ui;
                    println!("UI overlay: {}", if show_ui { "ON" } else { "OFF" });
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

fn draw_ui_overlay(
    window: &mut Window,
    font: &Arc<Font>,
    vertex_count: usize,
    edge_count: usize,
    face_count: usize,
    is_manifold: bool,
    hole_count: usize,
) {
    let x_offset = 11.0;
    let y_offset = 15.0;
    let line_height = 18.0;
    let header_size = 26.0;
    let text_size = 18.0;
    let header_padding = 8.0; // Extra padding after headers

    // Headers use lighter gray for a "thinner" appearance
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
        "Left Click+Drag: Rotate",
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    window.draw_text(
        "Right Click+Drag: Pan",
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    window.draw_text(
        "Scroll: Zoom",
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

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

    // Draw mesh statistics
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
        &format!("Vertices: {}", vertex_count),
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    window.draw_text(
        &format!("Edges: {}", edge_count),
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    window.draw_text(
        &format!("Faces: {}", face_count),
        &na::Point2::new(x_offset, current_y),
        text_size,
        font,
        &text_color,
    );
    current_y += line_height;

    // Manifold status with color
    let manifold_text = if is_manifold {
        "Manifold: Yes".to_string()
    } else {
        format!("Manifold: No ({} holes)", hole_count)
    };
    let manifold_color = if is_manifold {
        na::Point3::new(0.4, 1.0, 0.4) // Bright green
    } else {
        na::Point3::new(1.0, 0.4, 0.4) // Bright red
    };
    window.draw_text(
        &manifold_text,
        &na::Point2::new(10.0, current_y),
        text_size,
        font,
        &manifold_color,
    );
}
