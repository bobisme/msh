//! Geometry-only analysis. Render vertices (UVs, normals, skinning) are never welded.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Topology {
    pub vertices: usize,
    pub triangles: usize,
    pub edges: usize,
    pub boundary_edges: usize,
    pub boundary_loops: usize,
    pub boundary_components: usize,
    pub nonmanifold_edges: usize,
    pub nonmanifold_vertices: usize,
    pub inconsistent_winding_edges: usize,
    pub degenerate_triangles: usize,
    pub is_manifold: bool,
    pub is_closed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyReport {
    /// Absolute distance used only for analysis, not a render-geometry mutation.
    pub weld_tolerance: f64,
    pub raw: Topology,
    pub welded: Topology,
}

fn components(graph: &HashMap<u32, Vec<u32>>) -> (usize, usize) {
    let mut seen = HashSet::new();
    let mut count = 0;
    let mut loops = 0;
    for &start in graph.keys() {
        if !seen.insert(start) {
            continue;
        }
        count += 1;
        let mut cycle = true;
        let mut stack = vec![start];
        while let Some(vertex) = stack.pop() {
            let neighbors = &graph[&vertex];
            cycle &= neighbors.len() == 2;
            for &next in neighbors {
                if seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        loops += usize::from(cycle);
    }
    (count, loops)
}

fn analyze(vertex_count: usize, triangles: &[[u32; 3]]) -> Result<Topology, String> {
    let mut out = Topology {
        vertices: vertex_count,
        triangles: triangles.len(),
        ..Default::default()
    };
    let mut edges: HashMap<(u32, u32), (usize, i32)> = HashMap::new();
    let mut links: Vec<Vec<(u32, u32)>> = vec![Vec::new(); vertex_count];
    for &[a, b, c] in triangles {
        if [a, b, c].iter().any(|&v| v as usize >= vertex_count) {
            return Err("Triangle index exceeds vertex count".into());
        }
        if a == b || b == c || c == a {
            out.degenerate_triangles += 1;
            continue;
        }
        for (v, x, y) in [(a, b, c), (b, c, a), (c, a, b)] {
            links[v as usize].push((x, y));
            let edge = edges.entry((v.min(x), v.max(x))).or_default();
            edge.0 += 1;
            edge.1 += if v < x { 1 } else { -1 };
        }
    }
    let mut boundary: HashMap<u32, Vec<u32>> = HashMap::new();
    out.edges = edges.len();
    for ((a, b), (count, winding)) in edges {
        if count == 1 {
            out.boundary_edges += 1;
            boundary.entry(a).or_default().push(b);
            boundary.entry(b).or_default().push(a);
        }
        out.nonmanifold_edges += usize::from(count > 2);
        out.inconsistent_winding_edges += usize::from(count == 2 && winding != 0);
    }
    (out.boundary_components, out.boundary_loops) = components(&boundary);
    // The link of a manifold vertex is one cycle or one path, not multiple shells
    // touching at a point. Edge incidence alone would miss this bow-tie case.
    for link in links.iter().filter(|l| !l.is_empty()) {
        let mut graph: HashMap<u32, Vec<u32>> = HashMap::new();
        for &(a, b) in link {
            graph.entry(a).or_default().push(b);
            graph.entry(b).or_default().push(a);
        }
        let ends = graph.values().filter(|n| n.len() == 1).count();
        if graph.values().any(|n| n.len() > 2)
            || (ends != 0 && ends != 2)
            || components(&graph).0 != 1
        {
            out.nonmanifold_vertices += 1;
        }
    }
    out.is_manifold = out.nonmanifold_edges == 0
        && out.nonmanifold_vertices == 0
        && out.degenerate_triangles == 0;
    out.is_closed = out.is_manifold && out.boundary_edges == 0 && out.triangles > 0;
    Ok(out)
}

pub fn report(
    positions: &[[f32; 3]],
    triangles: &[[u32; 3]],
    tolerance: Option<f64>,
) -> Result<TopologyReport, String> {
    let raw = analyze(positions.len(), triangles)?;
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in positions {
        for i in 0..3 {
            if !p[i].is_finite() {
                return Err("Nonfinite mesh position".into());
            }
            min[i] = min[i].min(f64::from(p[i]));
            max[i] = max[i].max(f64::from(p[i]));
        }
    }
    let extent = (0..3).map(|i| max[i] - min[i]).fold(0.0, f64::max);
    let tolerance = tolerance.unwrap_or((extent * 1e-6).max(1e-9));
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err("Weld tolerance must be positive and finite".into());
    }
    let mut cells: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    let mut representatives: Vec<[f32; 3]> = Vec::new();
    let mut indices = Vec::with_capacity(positions.len());
    for p in positions {
        let mut cell = [0; 3];
        for i in 0..3 {
            let coordinate = ((f64::from(p[i]) - min[i]) / tolerance).floor();
            if coordinate > i64::MAX as f64 - 4096.0 {
                return Err("Weld tolerance too small for mesh extent".into());
            }
            cell[i] = coordinate as i64;
        }
        let mut found = None;
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if let Some(candidates) = cells.get(&[cell[0] + x, cell[1] + y, cell[2] + z]) {
                        for &candidate in candidates {
                            let q = representatives[candidate as usize];
                            let distance = (0..3)
                                .map(|i| (f64::from(p[i]) - f64::from(q[i])).powi(2))
                                .sum::<f64>();
                            if distance <= tolerance * tolerance {
                                found =
                                    Some(found.map_or(candidate, |old: u32| old.min(candidate)));
                            }
                        }
                    }
                }
            }
        }
        let index = found.unwrap_or_else(|| {
            let index = representatives.len() as u32;
            representatives.push(*p);
            cells.entry(cell).or_default().push(index);
            index
        });
        indices.push(index);
    }
    let welded_triangles: Vec<_> = triangles
        .iter()
        .map(|t| t.map(|v| indices[v as usize]))
        .collect();
    let welded = analyze(representatives.len(), &welded_triangles)?;
    Ok(TopologyReport {
        weld_tolerance: tolerance,
        raw,
        welded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seams_are_analyzed_without_modifying_render_vertices() {
        let tetra = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let positions: Vec<_> = faces.iter().flat_map(|t| t.map(|v| tetra[v])).collect();
        let indices = [[0, 1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11]];
        let result = report(&positions, &indices, None).unwrap();
        assert_eq!(result.raw.boundary_loops, 4);
        assert_eq!(result.welded.boundary_loops, 0);
        assert!(result.welded.is_closed);
        assert_eq!(positions.len(), 12);
    }
    #[test]
    fn boundaries_nonmanifold_vertices_and_bad_inputs_are_distinct() {
        let positions = [
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [-1., 0., 0.],
            [0., -1., 0.],
        ];
        let result = report(&positions, &[[0, 1, 2]], None).unwrap();
        assert!(result.raw.is_manifold && !result.raw.is_closed);
        assert_eq!(result.raw.boundary_loops, 1);
        let result = report(&positions, &[[0, 1, 2], [0, 3, 4]], None).unwrap();
        assert!(!result.raw.is_manifold);
        assert_eq!(result.raw.nonmanifold_vertices, 1);
        assert!(report(&positions, &[[0, 1, 99]], None).is_err());
        assert!(report(&positions, &[], Some(0.0)).is_err());
    }
}
