use nalgebra::Matrix4;

/// Iterative traversal accepts arbitrary node order, detects cycles and multiple
/// parents, and keeps deep imported hierarchies off the native stack.
pub fn world_transforms(
    document: &gltf::Document,
) -> Result<(Vec<Matrix4<f32>>, Vec<Option<usize>>), String> {
    let nodes: Vec<_> = document.nodes().collect();
    if nodes.len() > 250_000 {
        return Err("glTF exceeds 250000 nodes".into());
    }
    let mut parents = vec![None; nodes.len()];
    for node in &nodes {
        for child in node.children() {
            if parents[child.index()].replace(node.index()).is_some() {
                return Err("glTF node has multiple parents".into());
            }
        }
    }
    let mut states = vec![0u8; nodes.len()];
    let mut world = vec![Matrix4::identity(); nodes.len()];
    for start in 0..nodes.len() {
        if states[start] == 2 {
            continue;
        }
        let mut path = Vec::new();
        let mut cursor = Some(start);
        while let Some(index) = cursor {
            if states[index] == 2 {
                break;
            }
            if states[index] == 1 {
                return Err("Cycle in glTF node hierarchy".into());
            }
            states[index] = 1;
            path.push(index);
            cursor = parents[index];
        }
        while let Some(index) = path.pop() {
            let local = Matrix4::from(nodes[index].transform().matrix());
            world[index] = parents[index].map_or(local, |parent| world[parent] * local);
            if !world[index].iter().all(|v| v.is_finite()) {
                return Err("Nonfinite glTF node transform".into());
            }
            states[index] = 2;
        }
    }
    Ok((world, parents))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_before_parent_is_supported_and_cycles_are_rejected() {
        let make = |nodes: serde_json::Value| {
            gltf::Document::from_json(
                serde_json::from_value(
                    serde_json::json!({"asset":{"version":"2.0"},"nodes":nodes}),
                )
                .unwrap(),
            )
            .unwrap()
        };
        let document = make(
            serde_json::json!([{"translation":[1,0,0]},{"children":[0],"translation":[0,2,0]}]),
        );
        let (world, _) = world_transforms(&document).unwrap();
        assert_eq!(
            world[0].transform_point(&nalgebra::Point3::origin()),
            nalgebra::Point3::new(1.0, 2.0, 0.0)
        );
        let document = make(serde_json::json!([{"children":[1]},{"children":[0]}]));
        assert!(world_transforms(&document).is_err());
    }
}
