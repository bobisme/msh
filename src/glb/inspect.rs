use std::path::PathBuf;

/// Inspect GLB/glTF file structure
pub fn inspect_glb(path: &PathBuf, as_json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let (document, _buffers, _images) = gltf::import(path)?;

    if as_json {
        // JSON output
        let json_data = build_json_structure(&document)?;
        println!("{}", serde_json::to_string_pretty(&json_data)?);
    } else {
        for (scene_idx, scene) in document.scenes().enumerate() {
            println!(
                "Scene {}: {}",
                scene_idx,
                scene.name().unwrap_or("<unnamed>")
            );

            let nodes: Vec<_> = scene.nodes().collect();
            for (idx, node) in nodes.iter().enumerate() {
                let is_last = idx == nodes.len() - 1;
                print_node_tree(node, "", is_last);
            }
        }
    }

    Ok(())
}

fn print_node_tree(node: &gltf::Node, prefix: &str, is_last: bool) {
    let name = node.name().unwrap_or("<unnamed>");

    // Determine entity types
    let mut entity_types = Vec::new();

    if node.mesh().is_some() {
        entity_types.push("Mesh");
    }
    if node.camera().is_some() {
        entity_types.push("Camera");
    }
    entity_types.push("Transform");

    let type_str = entity_types.join(", ");

    // Print node info with proper connector
    let connector = if is_last { "└─" } else { "├─" };
    println!("{}{} {} [{}]", prefix, connector, name, type_str);

    // Determine the prefix for child content and check if node has children
    let children: Vec<_> = node.children().collect();
    let has_children = !children.is_empty();

    let child_prefix = if is_last {
        format!("{}   ", prefix)
    } else {
        format!("{}│  ", prefix)
    };

    // Use vertical line for properties when node has children
    let prop_prefix = if has_children {
        format!("{}│  ", child_prefix)
    } else {
        format!("{}   ", child_prefix)
    };

    // Print transform
    let (translation, rotation, scale) = node.transform().decomposed();
    println!(
        "{}Position: ({:.2}, {:.2}, {:.2})",
        prop_prefix, translation[0], translation[1], translation[2]
    );

    if scale != [1.0, 1.0, 1.0] {
        println!(
            "{}Scale: ({:.2}, {:.2}, {:.2})",
            prop_prefix, scale[0], scale[1], scale[2]
        );
    }

    if rotation != [0.0, 0.0, 0.0, 1.0] {
        println!(
            "{}Rotation: ({:.2}, {:.2}, {:.2}, {:.2})",
            prop_prefix, rotation[0], rotation[1], rotation[2], rotation[3]
        );
    }

    // Print custom properties if present
    let extras = node.extras();
    if let Some(extras_raw) = extras.as_ref() {
        if let Ok(extras_value) = serde_json::from_str::<serde_json::Value>(extras_raw.get()) {
            if let Some(obj) = extras_value.as_object() {
                if !obj.is_empty() {
                    println!("{}Custom Properties:", prop_prefix);
                    for (key, val) in obj {
                        println!("{}• {}: {}", prop_prefix, key, val);
                    }
                }
            }
        }
    }

    // Print mesh info if present
    if let Some(mesh) = node.mesh() {
        println!(
            "{}Mesh: {} ({} primitives)",
            prop_prefix,
            mesh.name().unwrap_or("<unnamed>"),
            mesh.primitives().count()
        );
    }

    // Recursively print children
    for (idx, child) in children.iter().enumerate() {
        let is_last_child = idx == children.len() - 1;
        print_node_tree(child, &child_prefix, is_last_child);
    }
}

fn build_json_structure(
    document: &gltf::Document,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut scenes = Vec::new();

    for scene in document.scenes() {
        let mut scene_data = serde_json::Map::new();
        scene_data.insert(
            "name".to_string(),
            serde_json::Value::String(scene.name().unwrap_or("<unnamed>").to_string()),
        );

        let mut nodes = Vec::new();
        for node in scene.nodes() {
            nodes.push(build_node_json(&node)?);
        }
        scene_data.insert("nodes".to_string(), serde_json::Value::Array(nodes));

        scenes.push(serde_json::Value::Object(scene_data));
    }

    Ok(serde_json::json!({
        "scenes": scenes
    }))
}

fn build_node_json(node: &gltf::Node) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut node_data = serde_json::Map::new();

    node_data.insert(
        "name".to_string(),
        serde_json::Value::String(node.name().unwrap_or("<unnamed>").to_string()),
    );

    // Add transform
    let (translation, rotation, scale) = node.transform().decomposed();
    node_data.insert(
        "transform".to_string(),
        serde_json::json!({
            "translation": translation,
            "rotation": rotation,
            "scale": scale,
        }),
    );

    // Add component types
    let mut types = Vec::new();
    if node.mesh().is_some() {
        types.push("Mesh");
    }
    if node.camera().is_some() {
        types.push("Camera");
    }
    types.push("Transform");
    node_data.insert(
        "types".to_string(),
        serde_json::Value::Array(
            types
                .into_iter()
                .map(|s| serde_json::Value::String(s.to_string()))
                .collect(),
        ),
    );

    // Add mesh info if present
    if let Some(mesh) = node.mesh() {
        node_data.insert(
            "mesh".to_string(),
            serde_json::json!({
                "name": mesh.name().unwrap_or("<unnamed>"),
                "primitive_count": mesh.primitives().count(),
            }),
        );
    }

    // Add custom properties if present
    let extras = node.extras();
    if let Some(extras_raw) = extras.as_ref() {
        if let Ok(extras_value) = serde_json::from_str::<serde_json::Value>(extras_raw.get()) {
            node_data.insert("extras".to_string(), extras_value);
        }
    }

    // Add children recursively
    let children: Vec<_> = node
        .children()
        .map(|child| build_node_json(&child))
        .collect::<Result<Vec<_>, _>>()?;

    if !children.is_empty() {
        node_data.insert("children".to_string(), serde_json::Value::Array(children));
    }

    Ok(serde_json::Value::Object(node_data))
}
