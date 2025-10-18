use baby_shark::algo::merge_points::merge_points;
use baby_shark::exports::nalgebra::Vector3;
use baby_shark::io::{write_to_file, Builder, IndexedBuilder};
use baby_shark::mesh::corner_table::CornerTableF;
use baby_shark::remeshing::incremental::IncrementalRemesher;
use baby_shark::remeshing::voxel::{MeshingMethod, VoxelRemesher};
use std::path::PathBuf;

use super::loader::load_mesh;

/// Merge vertices that are closer than tolerance
pub fn merge_close_vertices(
    mesh: &CornerTableF,
    tolerance: f32,
) -> Result<CornerTableF, Box<dyn std::error::Error>> {
    println!("Merging vertices with tolerance: {}", tolerance);

    let vertex_count_before = mesh.count_vertices();

    // Extract all vertex positions and build a VertexId -> index mapping
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut vertex_id_to_idx: std::collections::HashMap<_, usize> =
        std::collections::HashMap::new();

    for (idx, vertex_id) in mesh.vertices().enumerate() {
        let pos = mesh.vertex_position(vertex_id);
        positions.push([pos.x, pos.y, pos.z]);
        vertex_id_to_idx.insert(vertex_id, idx);
    }

    // Quantize positions to tolerance grid
    let inv_tolerance = 1.0 / tolerance;
    let quantized_positions: Vec<Vector3<f32>> = positions
        .iter()
        .map(|pos| {
            Vector3::new(
                (pos[0] * inv_tolerance).round() * tolerance,
                (pos[1] * inv_tolerance).round() * tolerance,
                (pos[2] * inv_tolerance).round() * tolerance,
            )
        })
        .collect();

    // Merge quantized vertices
    let merged = merge_points(quantized_positions.into_iter());

    println!(
        "Merged {} vertices into {} unique vertices",
        vertex_count_before,
        merged.points.len()
    );

    // Build vertex mapping: old vertex array index -> new vertex index
    let vertex_map: Vec<usize> = merged.indices;

    // Rebuild mesh with merged vertices
    let mut builder = CornerTableF::builder_indexed();
    builder.set_num_vertices(merged.points.len());

    for point in &merged.points {
        builder
            .add_vertex([point.x, point.y, point.z])
            .map_err(|e| format!("Failed to add vertex: {:?}", e))?;
    }

    // Add faces with remapped vertex indices
    let face_count = mesh.count_faces();
    builder.set_num_faces(face_count);

    for face_id in mesh.faces() {
        let (v0_id, v1_id, v2_id) = mesh.face_vertices(face_id);
        let v0_idx = vertex_id_to_idx[&v0_id];
        let v1_idx = vertex_id_to_idx[&v1_id];
        let v2_idx = vertex_id_to_idx[&v2_id];
        let v0 = vertex_map[v0_idx];
        let v1 = vertex_map[v1_idx];
        let v2 = vertex_map[v2_idx];

        // Skip degenerate faces (where vertices got merged into same point)
        if v0 != v1 && v1 != v2 && v0 != v2 {
            if let Err(e) = builder.add_face(v0, v1, v2) {
                // Skip faces that fail to add (likely degenerate)
                eprintln!("Warning: Skipping degenerate face: {:?}", e);
            }
        }
    }

    builder
        .finish()
        .map_err(|e| format!("Failed to build merged mesh: {:?}", e).into())
}

pub fn remesh_incremental(
    input: &PathBuf,
    output: &PathBuf,
    mesh_name: Option<&str>,
    iterations: u32,
    target_edge_length: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);
    let mut mesh = load_mesh(input, mesh_name)?;

    let vertex_count_before = mesh.count_vertices();
    let face_count_before = mesh.count_faces();

    println!(
        "Before remeshing: {} vertices, {} faces",
        vertex_count_before, face_count_before
    );
    println!(
        "Remeshing with {} iterations, target edge length: {}...",
        iterations, target_edge_length
    );

    // Convert u32 to u16 for iterations
    let iterations_u16 = iterations.min(u16::MAX as u32) as u16;

    let remesher = IncrementalRemesher::new()
        .with_iterations_count(iterations_u16)
        .with_split_edges(true)
        .with_collapse_edges(true)
        .with_flip_edges(true)
        .with_shift_vertices(true)
        .with_project_vertices(true);

    remesher.remesh(&mut mesh, target_edge_length);

    let vertex_count_after = mesh.count_vertices();
    let face_count_after = mesh.count_faces();

    println!(
        "After remeshing: {} vertices, {} faces",
        vertex_count_after, face_count_after
    );
    println!("Writing output to {:?}...", output);

    write_to_file(&mesh, output).map_err(|e| format!("Failed to write mesh: {:?}", e))?;

    println!("Done!");
    Ok(())
}

pub fn remesh_pipeline(
    input: &PathBuf,
    output: &PathBuf,
    mesh_name: Option<&str>,
    voxel_size: f32,
    tolerance: f32,
    no_fix: bool,
    iterations: u32,
    target_edge_length: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);
    let mut mesh = load_mesh(input, mesh_name)?;

    let vertex_count_initial = mesh.count_vertices();
    let face_count_initial = mesh.count_faces();

    println!(
        "Initial: {} vertices, {} faces",
        vertex_count_initial, face_count_initial
    );

    // Step 1: Fix the mesh (unless disabled)
    if !no_fix {
        println!("\n=== Step 1: Fixing Mesh ===");

        // Merge close vertices
        mesh = merge_close_vertices(&mesh, tolerance)?;
        println!(
            "After merging: {} vertices, {} faces",
            mesh.count_vertices(),
            mesh.count_faces()
        );

        // Check if mesh needs hole fixing
        let boundary_rings = mesh.boundary_rings();
        if !boundary_rings.is_empty() {
            println!("Found {} hole(s) in mesh", boundary_rings.len());
            println!(
                "Fixing holes using voxel remeshing (voxel size: {})...",
                voxel_size
            );

            let mut remesher = VoxelRemesher::default()
                .with_voxel_size(voxel_size)
                .with_meshing_method(MeshingMethod::Manifold);

            mesh = remesher.remesh(&mesh).ok_or("Voxel remeshing failed")?;

            println!(
                "After fixing: {} vertices, {} faces",
                mesh.count_vertices(),
                mesh.count_faces()
            );

            let boundary_rings_after = mesh.boundary_rings();
            if boundary_rings_after.is_empty() {
                println!("✓ Mesh is now manifold!");
            } else {
                println!("⚠ Warning: {} hole(s) remain", boundary_rings_after.len());
            }
        } else {
            println!("✓ Mesh is already manifold (no holes to fix)");
        }
    }

    // Step 2: Incremental remeshing
    println!("\n=== Step 2: Incremental Remeshing ===");
    println!(
        "Remeshing with {} iterations, target edge length: {}...",
        iterations, target_edge_length
    );

    let vertex_count_before_incremental = mesh.count_vertices();
    let face_count_before_incremental = mesh.count_faces();

    let iterations_u16 = iterations.min(u16::MAX as u32) as u16;

    let remesher = IncrementalRemesher::new()
        .with_iterations_count(iterations_u16)
        .with_split_edges(true)
        .with_collapse_edges(true)
        .with_flip_edges(true)
        .with_shift_vertices(true)
        .with_project_vertices(true);

    remesher.remesh(&mut mesh, target_edge_length);

    let vertex_count_final = mesh.count_vertices();
    let face_count_final = mesh.count_faces();

    println!(
        "After incremental remeshing: {} vertices, {} faces",
        vertex_count_final, face_count_final
    );

    // Final summary
    println!("\n=== Summary ===");
    println!(
        "Initial:  {} vertices, {} faces",
        vertex_count_initial, face_count_initial
    );
    if !no_fix {
        println!(
            "After fix: {} vertices, {} faces",
            vertex_count_before_incremental, face_count_before_incremental
        );
    }
    println!(
        "Final:    {} vertices, {} faces",
        vertex_count_final, face_count_final
    );

    println!("\nWriting output to {:?}...", output);
    write_to_file(&mesh, output).map_err(|e| format!("Failed to write mesh: {:?}", e))?;

    println!("Done!");
    Ok(())
}

#[derive(Clone, Debug)]
pub enum VoxelMethod {
    FeaturePreserving,
    Manifold,
}

pub fn remesh_voxel(
    input: &PathBuf,
    output: &PathBuf,
    mesh_name: Option<&str>,
    voxel_size: f32,
    method: VoxelMethod,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);
    let mesh = load_mesh(input, mesh_name)?;

    let vertex_count_before = mesh.count_vertices();
    let face_count_before = mesh.count_faces();

    println!(
        "Before remeshing: {} vertices, {} faces",
        vertex_count_before, face_count_before
    );
    println!(
        "Voxel remeshing with method: {:?}, voxel size: {}",
        method, voxel_size
    );

    let meshing_method = match method {
        VoxelMethod::FeaturePreserving => MeshingMethod::FeaturePreserving,
        VoxelMethod::Manifold => MeshingMethod::Manifold,
    };

    let mut remesher = VoxelRemesher::default()
        .with_voxel_size(voxel_size)
        .with_meshing_method(meshing_method);

    let remeshed_mesh = remesher.remesh(&mesh).ok_or("Voxel remeshing failed")?;

    let vertex_count_after = remeshed_mesh.count_vertices();
    let face_count_after = remeshed_mesh.count_faces();

    println!(
        "After remeshing: {} vertices, {} faces",
        vertex_count_after, face_count_after
    );

    // Check manifold status if using Manifold method
    if matches!(method, VoxelMethod::Manifold) {
        let boundary_rings = remeshed_mesh.boundary_rings();
        if boundary_rings.is_empty() {
            println!("✓ Output mesh is manifold (watertight)");
        } else {
            println!(
                "⚠ Warning: {} boundary ring(s) detected",
                boundary_rings.len()
            );
        }
    }

    println!("Writing output to {:?}...", output);
    write_to_file(&remeshed_mesh, output).map_err(|e| format!("Failed to write mesh: {:?}", e))?;

    println!("Done!");
    Ok(())
}

pub fn show_stats(input: &PathBuf, mesh_name: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);
    let mesh = load_mesh(input, mesh_name)?;

    println!("\n=== Mesh Statistics ===");
    println!("Vertices:  {}", mesh.count_vertices());
    println!("Faces:     {}", mesh.count_faces());
    println!("Triangles: {}", mesh.count_faces()); // For triangle meshes, faces = triangles
    println!("Edges:     {}", mesh.unique_edges().count());

    // Calculate bounding box
    let mut first = true;
    let mut min = [0.0f32, 0.0, 0.0];
    let mut max = [0.0f32, 0.0, 0.0];

    for vertex_id in mesh.vertices() {
        let pos = mesh.vertex_position(vertex_id);

        if first {
            min = [pos.x, pos.y, pos.z];
            max = [pos.x, pos.y, pos.z];
            first = false;
        } else {
            min[0] = min[0].min(pos.x);
            min[1] = min[1].min(pos.y);
            min[2] = min[2].min(pos.z);
            max[0] = max[0].max(pos.x);
            max[1] = max[1].max(pos.y);
            max[2] = max[2].max(pos.z);
        }
    }

    if !first {
        let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
        println!("\n=== Bounding Box ===");
        println!("Min: ({:.3}, {:.3}, {:.3})", min[0], min[1], min[2]);
        println!("Max: ({:.3}, {:.3}, {:.3})", max[0], max[1], max[2]);
        println!("Size: ({:.3}, {:.3}, {:.3})", size[0], size[1], size[2]);
    }

    Ok(())
}

pub fn check_manifold(
    input: &PathBuf,
    mesh_name: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);
    let mesh = load_mesh(input, mesh_name)?;

    println!("\n=== Manifold Check ===");
    println!("Analyzing mesh topology...\n");

    // Check boundary rings (holes)
    let boundary_rings = mesh.boundary_rings();

    if boundary_rings.is_empty() {
        println!("✓ Mesh is MANIFOLD (watertight)");
        println!("  No holes or boundaries detected.");
    } else {
        println!("✗ Mesh is NOT MANIFOLD");
        println!(
            "  Found {} boundary ring(s) (holes):\n",
            boundary_rings.len()
        );

        for (i, ring) in boundary_rings.iter().enumerate() {
            let mut edge_count = 0;
            mesh.boundary_edges(*ring, |_edge| {
                edge_count += 1;
                std::ops::ControlFlow::Continue(())
            });

            println!("  Hole {}: {} boundary edges", i + 1, edge_count);
        }

        println!("\nTo fix these holes, run:");
        println!("  msh fix {:?} --out <output.obj>", input);
    }

    Ok(())
}

pub fn fix_holes(
    input: &PathBuf,
    output: &PathBuf,
    mesh_name: Option<&str>,
    voxel_size: f32,
    tolerance: f32,
    no_merge: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading mesh from {:?}...", input);
    let mut mesh = load_mesh(input, mesh_name)?;

    let vertex_count_initial = mesh.count_vertices();
    let face_count_initial = mesh.count_faces();

    println!(
        "Initial: {} vertices, {} faces",
        vertex_count_initial, face_count_initial
    );

    // Merge close vertices first (unless disabled)
    if !no_merge {
        mesh = merge_close_vertices(&mesh, tolerance)?;
        println!(
            "After merging: {} vertices, {} faces",
            mesh.count_vertices(),
            mesh.count_faces()
        );
    }

    // Check if mesh needs fixing
    let boundary_rings = mesh.boundary_rings();
    if boundary_rings.is_empty() {
        println!("Mesh is already manifold (watertight). No fixing needed.");

        // Still write the output if we merged vertices
        if !no_merge && mesh.count_vertices() < vertex_count_initial {
            println!("Writing merged mesh to {:?}...", output);
            write_to_file(&mesh, output).map_err(|e| format!("Failed to write mesh: {:?}", e))?;
            println!("Done!");
        }

        return Ok(());
    }

    println!("Found {} hole(s) in mesh", boundary_rings.len());
    println!("Fixing holes using voxel remeshing...");
    println!("Voxel size: {}", voxel_size);

    // Use voxel remeshing with Manifold method to close holes
    let mut remesher = VoxelRemesher::default()
        .with_voxel_size(voxel_size)
        .with_meshing_method(MeshingMethod::Manifold);

    let fixed_mesh = remesher.remesh(&mesh).ok_or("Voxel remeshing failed")?;

    let vertex_count_after = fixed_mesh.count_vertices();
    let face_count_after = fixed_mesh.count_faces();

    println!(
        "After: {} vertices, {} faces",
        vertex_count_after, face_count_after
    );

    // Verify the result
    let boundary_rings_after = fixed_mesh.boundary_rings();
    if boundary_rings_after.is_empty() {
        println!("✓ Mesh is now manifold!");
    } else {
        println!(
            "⚠ Warning: {} hole(s) remain (may need smaller voxel size)",
            boundary_rings_after.len()
        );
    }

    println!("Writing output to {:?}...", output);
    write_to_file(&fixed_mesh, output).map_err(|e| format!("Failed to write mesh: {:?}", e))?;

    println!("Done!");
    Ok(())
}
