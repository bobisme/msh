//! Spike: explore gltf crate v1.4.1 skin data extraction
//!
//! Loads a skinned GLB (RiggedSimple.glb) and prints:
//!   - Joint hierarchy (parent-child, names, indices)
//!   - Inverse bind matrices
//!   - Per-vertex JOINTS_0 and WEIGHTS_0 data

use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "../test-assets/RiggedSimple.glb".to_string());

    println!("=== Loading: {} ===\n", path);
    let (document, buffers, _images) = gltf::import(&path)?;

    // Helper closure for buffer access
    let get_buf = |buffer: gltf::Buffer<'_>| Some(&*buffers[buffer.index()]);

    // --- 1. Enumerate skins ---
    println!("--- SKINS ---");
    let skins: Vec<_> = document.skins().collect();
    println!("Skin count: {}", skins.len());

    for skin in &skins {
        println!(
            "\nSkin #{}: name={:?}",
            skin.index(),
            skin.name().unwrap_or("<none>")
        );

        // --- 2. Joint list ---
        let joints: Vec<_> = skin.joints().collect();
        println!("  Joint count: {}", joints.len());

        // Build parent map: child_index -> parent_index
        // The gltf crate does NOT expose a parent() method on Node.
        // We must build it ourselves by walking children.
        let mut parent_map: HashMap<usize, usize> = HashMap::new();
        for node in document.nodes() {
            for child in node.children() {
                parent_map.insert(child.index(), node.index());
            }
        }

        println!("\n  Joint hierarchy:");
        for joint in &joints {
            let parent = parent_map.get(&joint.index());
            let children: Vec<usize> = joint.children().map(|c| c.index()).collect();
            let transform = joint.transform();
            let (t, r, s) = match transform {
                gltf::scene::Transform::Decomposed {
                    translation,
                    rotation,
                    scale,
                } => (translation, rotation, scale),
                gltf::scene::Transform::Matrix { matrix } => {
                    // For display, just show raw matrix
                    println!(
                        "    Joint #{} {:?} — transform is raw matrix: {:?}",
                        joint.index(),
                        joint.name().unwrap_or("<none>"),
                        matrix
                    );
                    continue;
                }
            };
            println!(
                "    Joint #{} {:?}  parent={:?}  children={:?}",
                joint.index(),
                joint.name().unwrap_or("<none>"),
                parent,
                children,
            );
            println!(
                "      translation={:?}  rotation={:?}  scale={:?}",
                t, r, s
            );
        }

        // --- 3. Skeleton root ---
        println!(
            "\n  Skeleton root: {:?}",
            skin.skeleton().map(|n| (n.index(), n.name()))
        );

        // --- 4. Inverse bind matrices ---
        let reader = skin.reader(get_buf);
        if let Some(ibm_iter) = reader.read_inverse_bind_matrices() {
            let ibms: Vec<[[f32; 4]; 4]> = ibm_iter.collect();
            println!("\n  Inverse bind matrices ({}):", ibms.len());
            for (i, mat) in ibms.iter().enumerate() {
                println!("    Joint #{}: [", i);
                for row in mat {
                    println!("      {:?}", row);
                }
                println!("    ]");
            }
        } else {
            println!("\n  Inverse bind matrices: NONE (identity assumed)");
        }
    }

    // --- 5. Per-vertex joint indices and weights ---
    println!("\n--- MESH PRIMITIVES: JOINTS_0 & WEIGHTS_0 ---");

    for node in document.nodes() {
        let skin_ref = node.skin();
        let mesh_ref = node.mesh();
        if let (Some(_skin), Some(mesh)) = (skin_ref, mesh_ref) {
            println!(
                "\nNode #{} {:?} -> Mesh {:?} (with skin)",
                node.index(),
                node.name().unwrap_or("<none>"),
                mesh.name().unwrap_or("<none>"),
            );

            for (pi, primitive) in mesh.primitives().enumerate() {
                println!("  Primitive #{}:", pi);

                let reader = primitive.reader(get_buf);

                // Positions count (for reference)
                let pos_count = reader
                    .read_positions()
                    .map(|p| p.count())
                    .unwrap_or(0);
                println!("    Vertex count: {}", pos_count);

                // JOINTS_0
                if let Some(joints) = reader.read_joints(0) {
                    let joints_u16: Vec<[u16; 4]> = joints.into_u16().collect();
                    println!("    JOINTS_0 count: {}", joints_u16.len());
                    let show = joints_u16.len().min(8);
                    for (i, j) in joints_u16.iter().take(show).enumerate() {
                        println!("      v{}: {:?}", i, j);
                    }
                    if joints_u16.len() > show {
                        println!("      ... ({} more)", joints_u16.len() - show);
                    }

                    // Check for JOINTS_1 (multi-set skinning)
                    let reader2 = primitive.reader(get_buf);
                    if reader2.read_joints(1).is_some() {
                        println!("    JOINTS_1: PRESENT (multi-set skinning)");
                    } else {
                        println!("    JOINTS_1: absent (single set only)");
                    }
                } else {
                    println!("    JOINTS_0: NONE");
                }

                // WEIGHTS_0
                let reader3 = primitive.reader(get_buf);
                if let Some(weights) = reader3.read_weights(0) {
                    let weights_f32: Vec<[f32; 4]> = weights.into_f32().collect();
                    println!("    WEIGHTS_0 count: {}", weights_f32.len());
                    let show = weights_f32.len().min(8);
                    for (i, w) in weights_f32.iter().take(show).enumerate() {
                        let sum: f32 = w.iter().sum();
                        println!("      v{}: {:?}  (sum={:.4})", i, w, sum);
                    }
                    if weights_f32.len() > show {
                        println!("      ... ({} more)", weights_f32.len() - show);
                    }
                } else {
                    println!("    WEIGHTS_0: NONE");
                }

                // Check accessor details for JOINTS_0
                let prim_ref = &primitive;
                if let Some(accessor) = prim_ref.get(&gltf::Semantic::Joints(0)) {
                    println!("\n    JOINTS_0 accessor details:");
                    println!("      data_type: {:?}", accessor.data_type());
                    println!("      dimensions: {:?}", accessor.dimensions());
                    println!("      count: {}", accessor.count());
                    println!(
                        "      sparse: {}",
                        if accessor.sparse().is_some() {
                            "YES"
                        } else {
                            "no"
                        }
                    );
                    println!(
                        "      normalized: {}",
                        accessor.normalized()
                    );
                }

                if let Some(accessor) = prim_ref.get(&gltf::Semantic::Weights(0)) {
                    println!("\n    WEIGHTS_0 accessor details:");
                    println!("      data_type: {:?}", accessor.data_type());
                    println!("      dimensions: {:?}", accessor.dimensions());
                    println!("      count: {}", accessor.count());
                    println!(
                        "      sparse: {}",
                        if accessor.sparse().is_some() {
                            "YES"
                        } else {
                            "no"
                        }
                    );
                    println!(
                        "      normalized: {}",
                        accessor.normalized()
                    );
                }
            }
        }
    }

    // --- 6. Check sparse accessor support ---
    println!("\n--- SPARSE ACCESSOR CHECK ---");
    let mut sparse_count = 0;
    for accessor in document.accessors() {
        if accessor.sparse().is_some() {
            sparse_count += 1;
            println!(
                "  Accessor #{}: sparse with {} entries",
                accessor.index(),
                accessor.sparse().unwrap().count()
            );
        }
    }
    if sparse_count == 0 {
        println!("  No sparse accessors in this file.");
    }
    println!("  (Note: gltf crate accessor::Iter handles sparse transparently via SparseIter)");

    // --- 7. Animation channels referencing skin joints ---
    println!("\n--- ANIMATIONS ---");
    for anim in document.animations() {
        println!(
            "Animation {:?}:",
            anim.name().unwrap_or("<none>")
        );
        for channel in anim.channels() {
            let target = channel.target();
            println!(
                "  Channel -> node #{} {:?}, property={:?}",
                target.node().index(),
                target.node().name().unwrap_or("<none>"),
                target.property(),
            );
        }
    }

    println!("\n=== Done ===");
    Ok(())
}
