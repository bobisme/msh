use nalgebra as na;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Closest point on triangle (bn-3gh)
// ---------------------------------------------------------------------------

/// Returns (closest_point, squared_distance) from `point` to triangle (a, b, c).
/// Handles all 7 Voronoi regions. Based on Ericson, Real-Time Collision Detection 5.1.5.
pub fn closest_point_on_triangle(
    point: &na::Point3<f32>,
    a: &na::Point3<f32>,
    b: &na::Point3<f32>,
    c: &na::Point3<f32>,
) -> (na::Point3<f32>, f32) {
    let ab = b - a;
    let ac = c - a;
    let ap = point - a;

    let d1 = ab.dot(&ap);
    let d2 = ac.dot(&ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return (*a, (point - a).norm_squared());
    }

    let bp = point - b;
    let d3 = ab.dot(&bp);
    let d4 = ac.dot(&bp);
    if d3 >= 0.0 && d4 <= d3 {
        return (*b, (point - b).norm_squared());
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        let closest = a + v * ab;
        return (closest, (point - closest).norm_squared());
    }

    let cp = point - c;
    let d5 = ab.dot(&cp);
    let d6 = ac.dot(&cp);
    if d6 >= 0.0 && d5 <= d6 {
        return (*c, (point - c).norm_squared());
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        let closest = a + w * ac;
        return (closest, (point - closest).norm_squared());
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let closest = b + w * (c - b);
        return (closest, (point - closest).norm_squared());
    }

    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    let closest = a + v * ab + w * ac;
    (closest, (point - closest).norm_squared())
}

// ---------------------------------------------------------------------------
// Spatial hash grid (bn-19y)
// ---------------------------------------------------------------------------

pub struct SpatialHashGrid {
    cell_size: f32,
    inv_cell_size: f32,
    cells: HashMap<(i32, i32, i32), Vec<usize>>,
    positions: Vec<na::Point3<f32>>,
    triangles: Vec<[u32; 3]>,
}

impl SpatialHashGrid {
    pub fn new(
        positions: &[na::Point3<f32>],
        triangles: &[[u32; 3]],
        cell_size: f32,
    ) -> Self {
        let inv_cell_size = 1.0 / cell_size;
        let mut cells: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();

        for (tri_idx, tri) in triangles.iter().enumerate() {
            let p0 = &positions[tri[0] as usize];
            let p1 = &positions[tri[1] as usize];
            let p2 = &positions[tri[2] as usize];

            // AABB of triangle
            let min_x = p0.x.min(p1.x).min(p2.x);
            let min_y = p0.y.min(p1.y).min(p2.y);
            let min_z = p0.z.min(p1.z).min(p2.z);
            let max_x = p0.x.max(p1.x).max(p2.x);
            let max_y = p0.y.max(p1.y).max(p2.y);
            let max_z = p0.z.max(p1.z).max(p2.z);

            let cx0 = (min_x * inv_cell_size).floor() as i32;
            let cy0 = (min_y * inv_cell_size).floor() as i32;
            let cz0 = (min_z * inv_cell_size).floor() as i32;
            let cx1 = (max_x * inv_cell_size).floor() as i32;
            let cy1 = (max_y * inv_cell_size).floor() as i32;
            let cz1 = (max_z * inv_cell_size).floor() as i32;

            for cx in cx0..=cx1 {
                for cy in cy0..=cy1 {
                    for cz in cz0..=cz1 {
                        cells.entry((cx, cy, cz)).or_default().push(tri_idx);
                    }
                }
            }
        }

        Self {
            cell_size,
            inv_cell_size,
            cells,
            positions: positions.to_vec(),
            triangles: triangles.to_vec(),
        }
    }

    fn cell_key(&self, point: &na::Point3<f32>) -> (i32, i32, i32) {
        (
            (point.x * self.inv_cell_size).floor() as i32,
            (point.y * self.inv_cell_size).floor() as i32,
            (point.z * self.inv_cell_size).floor() as i32,
        )
    }

    /// Find the closest point on the mesh surface to `point`.
    /// Returns (closest_point, squared_distance, triangle_index).
    pub fn query_nearest(
        &self,
        point: &na::Point3<f32>,
    ) -> (na::Point3<f32>, f32, usize) {
        let (cx, cy, cz) = self.cell_key(point);

        let mut best_point = na::Point3::origin();
        let mut best_dist_sq = f32::INFINITY;
        let mut best_tri = 0;
        let mut checked = Vec::new();

        // Expand search radius until we've covered enough
        for radius in 0..=64 {
            // If we already found something closer than the current ring, stop
            let ring_min_dist = if radius == 0 {
                0.0
            } else {
                ((radius - 1) as f32 * self.cell_size).powi(2)
            };
            if best_dist_sq < ring_min_dist {
                break;
            }

            let r = radius as i32;
            for dx in -r..=r {
                for dy in -r..=r {
                    for dz in -r..=r {
                        // Only check cells on the shell of this radius
                        if dx.abs() != r && dy.abs() != r && dz.abs() != r {
                            continue;
                        }
                        let key = (cx + dx, cy + dy, cz + dz);
                        if let Some(tris) = self.cells.get(&key) {
                            for &tri_idx in tris {
                                // Avoid re-checking triangles seen in inner rings
                                if checked.contains(&tri_idx) {
                                    continue;
                                }
                                checked.push(tri_idx);

                                let tri = &self.triangles[tri_idx];
                                let a = &self.positions[tri[0] as usize];
                                let b = &self.positions[tri[1] as usize];
                                let c = &self.positions[tri[2] as usize];
                                let (cp, dist_sq) =
                                    closest_point_on_triangle(point, a, b, c);
                                if dist_sq < best_dist_sq {
                                    best_dist_sq = dist_sq;
                                    best_point = cp;
                                    best_tri = tri_idx;
                                }
                            }
                        }
                    }
                }
            }
        }

        (best_point, best_dist_sq, best_tri)
    }
}

/// Compute the average edge length of a triangle mesh.
pub fn average_edge_length(positions: &[na::Point3<f32>], triangles: &[[u32; 3]]) -> f32 {
    if triangles.is_empty() {
        return 0.0;
    }
    let mut total = 0.0f32;
    let mut count = 0u32;
    for tri in triangles {
        let a = &positions[tri[0] as usize];
        let b = &positions[tri[1] as usize];
        let c = &positions[tri[2] as usize];
        total += (b - a).norm();
        total += (c - b).norm();
        total += (a - c).norm();
        count += 3;
    }
    total / count as f32
}

// ---------------------------------------------------------------------------
// Crease edge detection (bn-1mu)
// ---------------------------------------------------------------------------

/// Detect sharp crease edges on a mesh based on dihedral angle.
/// `crease_angle_deg` means "edges sharper than this many degrees from flat".
/// Returns line segments as `[endpoint_a, endpoint_b]`.
pub fn detect_crease_edges(
    positions: &[na::Point3<f32>],
    triangles: &[[u32; 3]],
    crease_angle_deg: f32,
) -> Vec<[na::Point3<f32>; 2]> {
    // Build edge -> adjacent face indices map
    let mut edge_faces: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (face_idx, tri) in triangles.iter().enumerate() {
        for i in 0..3 {
            let v0 = tri[i];
            let v1 = tri[(i + 1) % 3];
            let key = (v0.min(v1), v0.max(v1));
            edge_faces.entry(key).or_default().push(face_idx);
        }
    }

    let threshold = crease_angle_deg.to_radians().cos();
    let mut crease_edges = Vec::new();

    for (&(v0, v1), faces) in &edge_faces {
        // Only interior edges (exactly 2 adjacent faces)
        if faces.len() != 2 {
            continue;
        }

        let n0 = triangle_normal(positions, &triangles[faces[0]]);
        let n1 = triangle_normal(positions, &triangles[faces[1]]);

        // Skip degenerate triangles
        let (Some(n0), Some(n1)) = (n0, n1) else {
            continue;
        };

        // dot < cos(crease_angle) means the edge is sharper than crease_angle from flat
        if n0.dot(&n1) < threshold {
            crease_edges.push([positions[v0 as usize], positions[v1 as usize]]);
        }
    }

    crease_edges
}

fn triangle_normal(
    positions: &[na::Point3<f32>],
    tri: &[u32; 3],
) -> Option<na::Unit<na::Vector3<f32>>> {
    let a = &positions[tri[0] as usize];
    let b = &positions[tri[1] as usize];
    let c = &positions[tri[2] as usize];
    let cross = (b - a).cross(&(c - a));
    na::Unit::try_new(cross, 1e-10)
}

// ---------------------------------------------------------------------------
// Crease snapping (bn-311)
// ---------------------------------------------------------------------------

/// Find the closest point on any crease edge to `point`.
/// Returns (closest_point, squared_distance). Returns None if no crease edges.
pub fn nearest_crease_point(
    point: &na::Point3<f32>,
    crease_edges: &[[na::Point3<f32>; 2]],
) -> Option<(na::Point3<f32>, f32)> {
    if crease_edges.is_empty() {
        return None;
    }

    let mut best_point = crease_edges[0][0];
    let mut best_dist_sq = f32::INFINITY;

    for edge in crease_edges {
        let a = &edge[0];
        let b = &edge[1];
        let ab = b - a;
        let len_sq = ab.norm_squared();

        let closest = if len_sq < 1e-12 {
            // Degenerate edge (zero length)
            *a
        } else {
            let t = (point - a).dot(&ab) / len_sq;
            let t = t.clamp(0.0, 1.0);
            a + t * ab
        };

        let dist_sq = (point - closest).norm_squared();
        if dist_sq < best_dist_sq {
            best_dist_sq = dist_sq;
            best_point = closest;
        }
    }

    Some((best_point, best_dist_sq))
}

/// Snap a point to the nearest crease edge if within threshold distance.
pub fn snap_to_nearest_crease(
    point: &na::Point3<f32>,
    crease_edges: &[[na::Point3<f32>; 2]],
    snap_threshold: f32,
) -> Option<na::Point3<f32>> {
    let (closest, dist_sq) = nearest_crease_point(point, crease_edges)?;
    if dist_sq < snap_threshold * snap_threshold {
        Some(closest)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Projection loop (bn-1lz)
// ---------------------------------------------------------------------------

pub struct ProjectionConfig {
    pub iterations: usize,
    pub alpha: f32,
    pub beta: f32,
    pub snap_threshold: f32,
}

impl Default for ProjectionConfig {
    fn default() -> Self {
        Self {
            iterations: 10,
            alpha: 0.5,
            beta: 0.1,
            snap_threshold: 0.0, // caller should set from avg_edge_length
        }
    }
}

pub struct ProjectionStats {
    pub iterations_run: usize,
    pub avg_displacement: f32,
    pub max_displacement: f32,
    pub inversions_reverted: usize,
    pub vertices_snapped: usize,
}

/// Project simplified mesh vertices toward the reference surface.
///
/// Iteratively moves each vertex toward its closest point on the reference mesh,
/// blended with Laplacian smoothing. Optionally snaps vertices to crease edges.
/// Reverts any vertex moves that invert triangle normals.
pub fn project_to_reference(
    positions: &mut [na::Point3<f32>],
    triangles: &[[u32; 3]],
    reference_grid: &SpatialHashGrid,
    crease_edges: &[[na::Point3<f32>; 2]],
    config: &ProjectionConfig,
) -> ProjectionStats {
    let num_verts = positions.len();

    // Build vertex -> neighbor vertices map (from simplified mesh topology)
    let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); num_verts];
    for tri in triangles {
        for i in 0..3 {
            let v0 = tri[i] as usize;
            let v1 = tri[(i + 1) % 3] as usize;
            if !neighbors[v0].contains(&v1) {
                neighbors[v0].push(v1);
            }
            if !neighbors[v1].contains(&v0) {
                neighbors[v1].push(v0);
            }
        }
    }

    let initial_positions: Vec<na::Point3<f32>> = positions.to_vec();
    let mut total_inversions = 0usize;
    let mut total_snapped = 0usize;
    let mut iterations_run = 0;

    for _iter in 0..config.iterations {
        iterations_run += 1;
        let old_positions: Vec<na::Point3<f32>> = positions.to_vec();

        // Move each vertex
        for i in 0..num_verts {
            if neighbors[i].is_empty() {
                continue; // isolated vertex
            }

            // Closest point on reference surface
            let (target, _, _) = reference_grid.query_nearest(&positions[i]);

            // Laplacian: average of neighbors minus current position
            let avg_neighbor: na::Vector3<f32> = neighbors[i]
                .iter()
                .map(|&ni| positions[ni].coords)
                .sum::<na::Vector3<f32>>()
                / neighbors[i].len() as f32;
            let laplacian = avg_neighbor - positions[i].coords;

            let mut v_new =
                positions[i] + config.alpha * (target - positions[i]) + config.beta * laplacian;

            // Crease snapping
            if !crease_edges.is_empty() {
                if let Some(snapped) =
                    snap_to_nearest_crease(&v_new, crease_edges, config.snap_threshold)
                {
                    v_new = snapped;
                    total_snapped += 1;
                }
            }

            positions[i] = v_new;
        }

        // Check for inverted triangles, revert those vertices
        for tri in triangles {
            let old_n = triangle_normal_from_points(
                &old_positions[tri[0] as usize],
                &old_positions[tri[1] as usize],
                &old_positions[tri[2] as usize],
            );
            let new_n = triangle_normal_from_points(
                &positions[tri[0] as usize],
                &positions[tri[1] as usize],
                &positions[tri[2] as usize],
            );

            if let (Some(old_n), Some(new_n)) = (old_n, new_n) {
                if old_n.dot(&new_n) < 0.0 {
                    // Revert all 3 vertices
                    for &vi in tri {
                        positions[vi as usize] = old_positions[vi as usize];
                    }
                    total_inversions += 1;
                }
            }
        }

        // Early termination if converged
        let max_move_sq = positions
            .iter()
            .zip(old_positions.iter())
            .map(|(new, old)| (new - old).norm_squared())
            .fold(0.0f32, f32::max);
        if max_move_sq < 1e-12 {
            break;
        }
    }

    // Compute displacement stats
    let displacements: Vec<f32> = positions
        .iter()
        .zip(initial_positions.iter())
        .map(|(new, old)| (new - old).norm())
        .collect();
    let avg_displacement = displacements.iter().sum::<f32>() / num_verts as f32;
    let max_displacement = displacements.iter().cloned().fold(0.0f32, f32::max);

    ProjectionStats {
        iterations_run,
        avg_displacement,
        max_displacement,
        inversions_reverted: total_inversions,
        vertices_snapped: total_snapped,
    }
}

// ---------------------------------------------------------------------------
// Solver-based projection using del-msh-core (bn-1nh)
// ---------------------------------------------------------------------------

/// Project simplified mesh vertices toward the reference surface using
/// gradient descent with log-barrier collision avoidance and CCD line search.
///
/// This uses del_msh_core's collision-aware movement solver instead of
/// the heuristic Laplacian + inversion-revert approach.
pub fn project_with_solver(
    positions: &mut [na::Point3<f32>],
    triangles: &[[u32; 3]],
    reference_grid: &SpatialHashGrid,
    config: &ProjectionConfig,
) -> ProjectionStats {
    let num_verts = positions.len();
    let initial_positions: Vec<na::Point3<f32>> = positions.to_vec();

    // Convert triangles to flat usize array (del-msh format)
    let tri2vtx: Vec<usize> = triangles
        .iter()
        .flat_map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
        .collect();

    // Convert current positions to flat f64 array
    let vtx2xyz_start: Vec<f64> = positions
        .iter()
        .flat_map(|p| [p.x as f64, p.y as f64, p.z as f64])
        .collect();

    // Compute goal positions: closest point on reference mesh for each vertex
    let vtx2xyz_goal: Vec<f64> = positions
        .iter()
        .flat_map(|p| {
            let (target, _, _) = reference_grid.query_nearest(p);
            [target.x as f64, target.y as f64, target.z as f64]
        })
        .collect();

    // Deduplicate triangles (some remeshers produce duplicate faces sharing all 3 verts)
    let tri2vtx = dedup_triangles(&tri2vtx);

    // Check for existing self-intersections in the start mesh
    let start_intersections = std::panic::catch_unwind(|| {
        del_msh_core::trimesh3_intersection::search_brute_force(&tri2vtx, &vtx2xyz_start)
    });

    let has_intersections = match &start_intersections {
        Ok(pairs) => !pairs.is_empty(),
        Err(_) => true, // panic in intersection check — treat as having intersections
    };

    let result_vtx2xyz = if has_intersections {
        // Start mesh has self-intersections from QEM — resolve them first by
        // slightly shrinking each vertex toward the mesh centroid, then run
        // the collision-aware solver.
        let count = start_intersections.as_ref().map(|p| p.len()).unwrap_or(0);
        eprintln!(
            "Note: start mesh has {} self-intersections, resolving before projection",
            count
        );

        // Try to resolve intersections by using match_vtx2xyz_while_avoid_collision
        // with the start positions as the goal (identity movement) and a contact barrier.
        // This nudges intersecting vertices apart.
        let edge2vtx =
            del_msh_core::edge2vtx::from_triangle_mesh(&tri2vtx, num_verts);
        let avg_edge = compute_avg_edge_length_f64(&vtx2xyz_start, &tri2vtx);

        // First pass: try to untangle by projecting toward goal with a small step
        // The goal positions (on the reference surface) are intersection-free by definition,
        // so moving toward them should help resolve self-intersections.
        let blend = 0.3;
        let mut vtx2xyz_resolved: Vec<f64> = vtx2xyz_start
            .iter()
            .zip(vtx2xyz_goal.iter())
            .map(|(&s, &g)| s + blend * (g - s))
            .collect();

        // Check if resolved mesh is intersection-free
        let resolved_ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            del_msh_core::trimesh3_intersection::search_brute_force(&tri2vtx, &vtx2xyz_resolved)
        }));

        match resolved_ok {
            Ok(pairs) if pairs.is_empty() => {
                eprintln!("  Resolved intersections via blend, running collision-aware solver");
                let params = del_msh_core::trimesh3_move_avoid_intersection::Params {
                    k_diff: 1.0,
                    k_contact: 100.0,
                    dist0: avg_edge * 0.5,
                    alpha: config.alpha as f64,
                    num_iter: config.iterations,
                };
                del_msh_core::trimesh3_move_avoid_intersection::match_vtx2xyz_while_avoid_collision(
                    &tri2vtx,
                    &vtx2xyz_resolved,
                    &vtx2xyz_goal,
                    params,
                )
            }
            _ => {
                // Still has intersections after blend — just use the blended result
                eprintln!("  Could not fully resolve intersections, using blended projection");
                vtx2xyz_resolved
            }
        }
    } else {
        // Use collision-aware solver
        let params = del_msh_core::trimesh3_move_avoid_intersection::Params {
            k_diff: 1.0,
            k_contact: 100.0,
            dist0: compute_avg_edge_length_f64(&vtx2xyz_start, &tri2vtx) * 0.5,
            alpha: config.alpha as f64,
            num_iter: config.iterations,
        };
        del_msh_core::trimesh3_move_avoid_intersection::match_vtx2xyz_while_avoid_collision(
            &tri2vtx,
            &vtx2xyz_start,
            &vtx2xyz_goal,
            params,
        )
    };

    // Convert back to Point3<f32>
    for (i, p) in positions.iter_mut().enumerate() {
        p.x = result_vtx2xyz[i * 3] as f32;
        p.y = result_vtx2xyz[i * 3 + 1] as f32;
        p.z = result_vtx2xyz[i * 3 + 2] as f32;
    }

    // Compute displacement stats
    let displacements: Vec<f32> = positions
        .iter()
        .zip(initial_positions.iter())
        .map(|(new, old)| (new - old).norm())
        .collect();
    let avg_displacement = displacements.iter().sum::<f32>() / num_verts as f32;
    let max_displacement = displacements.iter().cloned().fold(0.0f32, f32::max);

    ProjectionStats {
        iterations_run: config.iterations,
        avg_displacement,
        max_displacement,
        inversions_reverted: 0,
        vertices_snapped: 0,
    }
}

/// Remove duplicate triangles (same 3 vertex indices, any winding order).
fn dedup_triangles(tri2vtx: &[usize]) -> Vec<usize> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(tri2vtx.len());
    for tri in tri2vtx.chunks(3) {
        let mut sorted = [tri[0], tri[1], tri[2]];
        sorted.sort();
        if seen.insert((sorted[0], sorted[1], sorted[2])) {
            result.extend_from_slice(tri);
        }
    }
    result
}

fn compute_avg_edge_length_f64(vtx2xyz: &[f64], tri2vtx: &[usize]) -> f64 {
    let mut total = 0.0f64;
    let mut count = 0u32;
    for tri in tri2vtx.chunks(3) {
        for i in 0..3 {
            let v0 = tri[i];
            let v1 = tri[(i + 1) % 3];
            let dx = vtx2xyz[v0 * 3] - vtx2xyz[v1 * 3];
            let dy = vtx2xyz[v0 * 3 + 1] - vtx2xyz[v1 * 3 + 1];
            let dz = vtx2xyz[v0 * 3 + 2] - vtx2xyz[v1 * 3 + 2];
            total += (dx * dx + dy * dy + dz * dz).sqrt();
            count += 3;
        }
    }
    total / count as f64
}

fn triangle_normal_from_points(
    a: &na::Point3<f32>,
    b: &na::Point3<f32>,
    c: &na::Point3<f32>,
) -> Option<na::Vector3<f32>> {
    let cross = (b - a).cross(&(c - a));
    let len = cross.norm();
    if len < 1e-10 {
        None
    } else {
        Some(cross / len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32, z: f32) -> na::Point3<f32> {
        na::Point3::new(x, y, z)
    }

    // -- closest_point_on_triangle tests --

    #[test]
    fn test_closest_point_interior() {
        let a = pt(0.0, 0.0, 0.0);
        let b = pt(1.0, 0.0, 0.0);
        let c = pt(0.0, 1.0, 0.0);
        let p = pt(0.2, 0.2, 1.0);
        let (closest, dist_sq) = closest_point_on_triangle(&p, &a, &b, &c);
        assert!((closest.z - 0.0).abs() < 1e-6);
        assert!((dist_sq - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_closest_point_vertex_a() {
        let a = pt(0.0, 0.0, 0.0);
        let b = pt(1.0, 0.0, 0.0);
        let c = pt(0.0, 1.0, 0.0);
        let p = pt(-1.0, -1.0, 0.0);
        let (closest, _) = closest_point_on_triangle(&p, &a, &b, &c);
        assert!((closest - a).norm() < 1e-6);
    }

    #[test]
    fn test_closest_point_vertex_b() {
        let a = pt(0.0, 0.0, 0.0);
        let b = pt(1.0, 0.0, 0.0);
        let c = pt(0.0, 1.0, 0.0);
        let p = pt(2.0, -1.0, 0.0);
        let (closest, _) = closest_point_on_triangle(&p, &a, &b, &c);
        assert!((closest - b).norm() < 1e-6);
    }

    #[test]
    fn test_closest_point_vertex_c() {
        let a = pt(0.0, 0.0, 0.0);
        let b = pt(1.0, 0.0, 0.0);
        let c = pt(0.0, 1.0, 0.0);
        let p = pt(-1.0, 2.0, 0.0);
        let (closest, _) = closest_point_on_triangle(&p, &a, &b, &c);
        assert!((closest - c).norm() < 1e-6);
    }

    #[test]
    fn test_closest_point_edge_ab() {
        let a = pt(0.0, 0.0, 0.0);
        let b = pt(1.0, 0.0, 0.0);
        let c = pt(0.0, 1.0, 0.0);
        let p = pt(0.5, -1.0, 0.0);
        let (closest, _) = closest_point_on_triangle(&p, &a, &b, &c);
        assert!((closest.y - 0.0).abs() < 1e-6);
        assert!(closest.x > 0.0 && closest.x < 1.0);
    }

    #[test]
    fn test_closest_point_edge_bc() {
        let a = pt(0.0, 0.0, 0.0);
        let b = pt(1.0, 0.0, 0.0);
        let c = pt(0.0, 1.0, 0.0);
        let p = pt(1.0, 1.0, 0.0);
        let (closest, _) = closest_point_on_triangle(&p, &a, &b, &c);
        // Should be on the BC edge (x + y = 1)
        assert!((closest.x + closest.y - 1.0).abs() < 1e-5);
    }

    // -- crease detection tests --

    #[test]
    fn test_crease_cube_edges() {
        // Two triangles forming a 90-degree dihedral (like a cube edge)
        let positions = vec![
            pt(0.0, 0.0, 0.0),
            pt(1.0, 0.0, 0.0),
            pt(0.0, 1.0, 0.0),
            pt(0.0, 0.0, 1.0),
        ];
        // Two faces sharing edge (0, 1): one in XY plane, one in XZ plane
        let triangles = vec![[0, 1, 2], [0, 3, 1]];

        // 90 degrees is sharper than 45 from flat
        let edges = detect_crease_edges(&positions, &triangles, 45.0);
        assert_eq!(edges.len(), 1);

        // 90 degrees is not sharper than 91 from flat
        let edges = detect_crease_edges(&positions, &triangles, 91.0);
        assert_eq!(edges.len(), 0);
    }

    #[test]
    fn test_crease_coplanar() {
        // Two coplanar triangles (0-degree dihedral from flat)
        let positions = vec![
            pt(0.0, 0.0, 0.0),
            pt(1.0, 0.0, 0.0),
            pt(0.0, 1.0, 0.0),
            pt(1.0, 1.0, 0.0),
        ];
        let triangles = vec![[0, 1, 2], [1, 3, 2]];
        let edges = detect_crease_edges(&positions, &triangles, 1.0);
        assert_eq!(edges.len(), 0);
    }

    // -- crease snapping tests --

    #[test]
    fn test_snap_within_threshold() {
        let edges = vec![[pt(0.0, 0.0, 0.0), pt(1.0, 0.0, 0.0)]];
        let p = pt(0.5, 0.1, 0.0);
        let result = snap_to_nearest_crease(&p, &edges, 0.5);
        assert!(result.is_some());
        let snapped = result.unwrap();
        assert!((snapped.x - 0.5).abs() < 1e-6);
        assert!((snapped.y - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_snap_beyond_threshold() {
        let edges = vec![[pt(0.0, 0.0, 0.0), pt(1.0, 0.0, 0.0)]];
        let p = pt(0.5, 2.0, 0.0);
        let result = snap_to_nearest_crease(&p, &edges, 0.5);
        assert!(result.is_none());
    }

    #[test]
    fn test_snap_to_endpoint() {
        let edges = vec![[pt(0.0, 0.0, 0.0), pt(1.0, 0.0, 0.0)]];
        let p = pt(-0.1, 0.0, 0.0);
        let result = snap_to_nearest_crease(&p, &edges, 0.5);
        assert!(result.is_some());
        let snapped = result.unwrap();
        assert!((snapped.x - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_snap_empty_edges() {
        let edges: Vec<[na::Point3<f32>; 2]> = vec![];
        let p = pt(0.0, 0.0, 0.0);
        assert!(snap_to_nearest_crease(&p, &edges, 1.0).is_none());
    }

    // -- spatial hash tests --

    #[test]
    fn test_spatial_hash_matches_brute_force() {
        let positions = vec![
            pt(0.0, 0.0, 0.0),
            pt(1.0, 0.0, 0.0),
            pt(0.0, 1.0, 0.0),
            pt(1.0, 1.0, 0.0),
            pt(0.5, 0.5, 1.0),
        ];
        let triangles = vec![[0, 1, 2], [1, 3, 2], [0, 1, 4], [1, 3, 4]];

        let grid = SpatialHashGrid::new(&positions, &triangles, 0.5);

        // Test several query points
        let queries = vec![
            pt(0.25, 0.25, 0.5),
            pt(0.5, 0.5, 0.0),
            pt(2.0, 2.0, 2.0),
            pt(0.0, 0.0, 0.0),
        ];

        for q in &queries {
            let (grid_pt, grid_dist, _) = grid.query_nearest(q);

            // Brute force
            let mut best_dist = f32::INFINITY;
            let mut best_pt = na::Point3::origin();
            for tri in &triangles {
                let a = &positions[tri[0] as usize];
                let b = &positions[tri[1] as usize];
                let c = &positions[tri[2] as usize];
                let (cp, d) = closest_point_on_triangle(q, a, b, c);
                if d < best_dist {
                    best_dist = d;
                    best_pt = cp;
                }
            }

            assert!(
                (grid_dist - best_dist).abs() < 1e-5,
                "Distance mismatch for query {:?}: grid={} brute={}",
                q,
                grid_dist,
                best_dist
            );
            assert!(
                (grid_pt - best_pt).norm() < 1e-5,
                "Point mismatch for query {:?}",
                q
            );
        }
    }
}
