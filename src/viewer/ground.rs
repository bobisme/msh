use nalgebra as na;

/// Rest-pose mesh bounds; the shadow catcher stays still during animation.
#[derive(Clone, Copy, Debug)]
pub struct GroundBounds {
    min: [f32; 3],
    max: [f32; 3],
}

impl GroundBounds {
    pub fn from_positions(positions: impl IntoIterator<Item = [f32; 3]>) -> Option<Self> {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for position in positions {
            if !position.iter().all(|v| v.is_finite()) {
                return None;
            }
            for i in 0..3 {
                min[i] = min[i].min(position[i]);
                max[i] = max[i].max(position[i]);
            }
        }
        min[0].is_finite().then_some(Self { min, max })
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min: std::array::from_fn(|i| self.min[i].min(other.min[i])),
            max: std::array::from_fn(|i| self.max[i].max(other.max[i])),
        }
    }

    pub fn transformed(&self, model: &na::Matrix4<f32>) -> Option<Self> {
        let corners = (0..8).map(|bits| {
            let p = na::Point3::new(
                if bits & 1 == 0 {
                    self.min[0]
                } else {
                    self.max[0]
                },
                if bits & 2 == 0 {
                    self.min[1]
                } else {
                    self.max[1]
                },
                if bits & 4 == 0 {
                    self.min[2]
                } else {
                    self.max[2]
                },
            );
            model.transform_point(&p).coords.into()
        });
        Self::from_positions(corners)
    }

    /// World-horizontal catcher below the transformed bounding box, including
    /// rotations and negative scales. Returns center x, height, center z, radius.
    pub fn placement(&self, model: &na::Matrix4<f32>) -> [f32; 4] {
        let Some(world) = self.transformed(model) else {
            return [0.0; 4];
        };
        let size: [f32; 3] = std::array::from_fn(|i| world.max[i] - world.min[i]);
        let scale = size.into_iter().fold(0.0001_f32, f32::max);
        [
            (world.min[0] + world.max[0]) * 0.5,
            world.min[1] - scale * 0.002,
            (world.min[2] + world.max[2]) * 0.5,
            scale * 4.0,
        ]
    }

    /// Fit a directional shadow camera to the mesh. The receiver has the same
    /// light-space x/y as its caster, so only the depth range needs extra room.
    pub fn shadow_view_projection(
        &self,
        model: &na::Matrix4<f32>,
        direction: [f32; 3],
    ) -> Option<na::Matrix4<f32>> {
        let light = na::Vector3::from(direction).try_normalize(0.00001)?;
        // No ground shadow when the key light is below or almost on the horizon.
        if !light.iter().all(|v| v.is_finite()) || light.y <= 0.1 {
            return None;
        }
        let world = self.transformed(model)?;
        let min = na::Point3::from(world.min);
        let max = na::Point3::from(world.max);
        let center = na::Point3::from((min.coords + max.coords) * 0.5);
        let size = max - min;
        let scale = size.max().max(0.0001);
        let radius = size.norm().max(0.0001) * 0.6;
        let up = if light.y.abs() > 0.95 {
            na::Vector3::z()
        } else {
            na::Vector3::y()
        };
        let view = na::Matrix4::look_at_rh(&(center + light * scale * 4.0), &center, &up);
        let mut proj = na::Matrix4::new_orthographic(
            -radius,
            radius,
            -radius,
            radius,
            scale * 0.01,
            scale * 16.0,
        );
        // nalgebra uses OpenGL depth; wgpu needs [0, 1].
        proj[(2, 2)] *= 0.5;
        proj[(2, 3)] = proj[(2, 3)] * 0.5 + 0.5;
        Some(proj * view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_remains_below_scaled_rotated_and_offset_mesh() {
        let bounds = GroundBounds::from_positions([[2.0, 3.0, 4.0], [4.0, 9.0, 8.0]]).unwrap();
        let model = na::Translation3::new(10.0, -5.0, 2.0).to_homogeneous()
            * na::Rotation3::from_euler_angles(0.7, 0.4, 0.3).to_homogeneous()
            * na::Matrix4::new_scaling(-2.0);
        let plane = bounds.placement(&model);
        for x in [2.0, 4.0] {
            for y in [3.0, 9.0] {
                for z in [4.0, 8.0] {
                    let p = model.transform_point(&na::Point3::new(x, y, z));
                    assert!(p.y > plane[1]);
                    assert!((p.x - plane[0]).abs() < plane[3]);
                    assert!((p.z - plane[2]).abs() < plane[3]);
                }
            }
        }
    }

    #[test]
    fn empty_invalid_and_flat_meshes_have_safe_placements() {
        assert!(GroundBounds::from_positions([]).is_none());
        assert!(GroundBounds::from_positions([[f32::NAN, 0.0, 0.0]]).is_none());
        let bounds = GroundBounds::from_positions([[0.0; 3]]).unwrap();
        let plane = bounds.placement(&na::Matrix4::identity());
        assert!(plane[1] < 0.0 && plane[3] > 0.0);
    }

    #[test]
    fn shadow_camera_contains_casters_and_their_ground_projections() {
        let bounds = GroundBounds::from_positions([[-1.0; 3], [1.0; 3]]).unwrap();
        let model =
            na::Translation3::new(8.0, -3.0, 12.0).to_homogeneous() * na::Matrix4::new_scaling(2.0);
        let plane = bounds.placement(&model);
        for direction in [[0.5, 1.0, 0.5], [0.0, 1.0, 0.0], [1.0, 0.15, 0.0]] {
            let matrix = bounds.shadow_view_projection(&model, direction).unwrap();
            let light = na::Vector3::from(direction).normalize();
            for x in [-1.0, 1.0] {
                for y in [-1.0, 1.0] {
                    for z in [-1.0, 1.0] {
                        let caster = model.transform_point(&na::Point3::new(x, y, z));
                        let receiver = caster - light * ((caster.y - plane[1]) / light.y);
                        for p in [caster, receiver] {
                            let clip = matrix.transform_point(&p);
                            assert!(clip.x.abs() < 1.0 && clip.y.abs() < 1.0);
                            assert!((0.0..1.0).contains(&clip.z));
                        }
                    }
                }
            }
        }
        for direction in [[0.0; 3], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0]] {
            assert!(bounds.shadow_view_projection(&model, direction).is_none());
        }
    }
}
