use nalgebra::{Matrix4, Quaternion, UnitQuaternion, Vector3};

/// A single joint in a skeleton hierarchy.
pub struct Joint {
    /// Index in the skeleton's joints array (NOT the glTF node index).
    pub index: usize,
    /// The glTF node index for this joint.
    pub node_index: usize,
    /// Optional joint name from the glTF file.
    pub name: Option<String>,
    /// Index (in the joints array) of this joint's parent, if any.
    pub parent: Option<usize>,
    /// Inverse bind matrix for this joint (column-major, as stored in glTF).
    pub inverse_bind_matrix: [[f32; 4]; 4],
    /// Local transform of this joint's node.
    pub local_transform: JointTransform,
}

/// Local transform for a joint node, matching glTF's two representations.
pub enum JointTransform {
    /// Translation, rotation (quaternion xyzw), and scale.
    Decomposed {
        translation: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
    /// Raw 4x4 matrix (column-major).
    Matrix([[f32; 4]; 4]),
}

/// A skeleton extracted from a glTF skin.
pub struct Skeleton {
    /// Ordered list of joints (index in this vec == joint index used in JOINTS_0).
    pub joints: Vec<Joint>,
}

impl JointTransform {
    /// Decompose into (translation, rotation_xyzw, scale).
    ///
    /// For `Decomposed` variants this is trivial. For `Matrix` variants we
    /// extract T/R/S from the matrix (assuming no skew).
    pub fn decompose(&self) -> ([f32; 3], [f32; 4], [f32; 3]) {
        match self {
            JointTransform::Decomposed {
                translation,
                rotation,
                scale,
            } => (*translation, *rotation, *scale),
            JointTransform::Matrix(m) => {
                let mat = Matrix4::from(*m);
                let translation = [mat[(0, 3)], mat[(1, 3)], mat[(2, 3)]];
                // Extract scale from column lengths
                let sx = Vector3::new(mat[(0, 0)], mat[(1, 0)], mat[(2, 0)]).norm();
                let sy = Vector3::new(mat[(0, 1)], mat[(1, 1)], mat[(2, 1)]).norm();
                let sz = Vector3::new(mat[(0, 2)], mat[(1, 2)], mat[(2, 2)]).norm();
                let scale = [sx, sy, sz];
                // Extract rotation matrix (normalised columns)
                let rot_mat = nalgebra::Matrix3::new(
                    mat[(0, 0)] / sx, mat[(0, 1)] / sy, mat[(0, 2)] / sz,
                    mat[(1, 0)] / sx, mat[(1, 1)] / sy, mat[(1, 2)] / sz,
                    mat[(2, 0)] / sx, mat[(2, 1)] / sy, mat[(2, 2)] / sz,
                );
                let rot = nalgebra::Rotation3::from_matrix_unchecked(rot_mat);
                let q = UnitQuaternion::from_rotation_matrix(&rot);
                let qi = q.into_inner();
                let rotation = [qi.i, qi.j, qi.k, qi.w];
                (translation, rotation, scale)
            }
        }
    }

    /// Convert this local transform to a nalgebra Matrix4.
    fn to_matrix4(&self) -> Matrix4<f32> {
        match self {
            JointTransform::Decomposed {
                translation,
                rotation,
                scale,
            } => {
                // glTF quaternion is [x, y, z, w]
                let quat = UnitQuaternion::new_normalize(Quaternion::new(
                    rotation[3],
                    rotation[0],
                    rotation[1],
                    rotation[2],
                ));
                let t = Matrix4::new_translation(&Vector3::new(
                    translation[0],
                    translation[1],
                    translation[2],
                ));
                let r = quat.to_homogeneous();
                let s = Matrix4::new_nonuniform_scaling(&Vector3::new(
                    scale[0], scale[1], scale[2],
                ));
                // TRS order: translate * rotate * scale
                t * r * s
            }
            JointTransform::Matrix(m) => {
                // Column-major [[f32;4];4] where each [f32;4] is a column — matches nalgebra storage.
                Matrix4::from(*m)
            }
        }
    }
}

impl Skeleton {
    /// Compute the world-space transform for every joint by walking the hierarchy root-to-leaf.
    ///
    /// Assumes joints are stored so that each joint's parent has a lower index than the joint
    /// itself (guaranteed by well-formed glTF files). A single forward pass over the array
    /// is therefore sufficient to compute all world transforms.
    pub fn compute_world_transforms(&self) -> Vec<[[f32; 4]; 4]> {
        self.compute_world_transforms_from(None)
    }

    /// Compute world transforms using the provided local transforms instead of the skeleton's
    /// rest-pose transforms. If `local_transforms` does not contain an entry for a joint
    /// (i.e. the slice is shorter), the joint's rest-pose local transform is used as fallback.
    fn compute_world_transforms_from(
        &self,
        local_transforms: Option<&[[[f32; 4]; 4]]>,
    ) -> Vec<[[f32; 4]; 4]> {
        let n = self.joints.len();
        let mut world: Vec<Matrix4<f32>> = Vec::with_capacity(n);

        for (i, joint) in self.joints.iter().enumerate() {
            let local = match local_transforms {
                Some(lt) if i < lt.len() => Matrix4::from(lt[i]),
                _ => joint.local_transform.to_matrix4(),
            };

            let world_transform = match joint.parent {
                Some(parent_idx) => {
                    // Parent is guaranteed to have a lower index, so it's already computed.
                    debug_assert!(parent_idx < i, "parent index must be less than child index");
                    world[parent_idx] * local
                }
                None => local,
            };

            world.push(world_transform);
        }

        world.iter().map(|m| (*m).into()).collect()
    }

    /// Compute the joint matrices needed for GPU skinning (bind-pose).
    ///
    /// For each joint: `joint_matrix = world_transform * inverse_bind_matrix`.
    /// The result is a flat array of column-major mat4s indexed by joint index,
    /// ready to be uploaded to a GPU uniform buffer.
    pub fn compute_joint_matrices(&self) -> Vec<[[f32; 4]; 4]> {
        let world_transforms = self.compute_world_transforms();
        self.apply_inverse_bind_matrices(&world_transforms)
    }

    /// Compute joint matrices using externally-provided local transforms (e.g. from animation).
    ///
    /// `local_transforms` is a slice of column-major mat4s, one per joint.
    /// If the slice is shorter than the joint count, remaining joints use their rest-pose
    /// local transform as fallback. This is what the animation system calls each frame
    /// with interpolated transforms.
    pub fn compute_joint_matrices_with_pose(
        &self,
        local_transforms: &[[[f32; 4]; 4]],
    ) -> Vec<[[f32; 4]; 4]> {
        let world_transforms = self.compute_world_transforms_from(Some(local_transforms));
        self.apply_inverse_bind_matrices(&world_transforms)
    }

    /// Multiply each world transform by the corresponding inverse bind matrix.
    fn apply_inverse_bind_matrices(
        &self,
        world_transforms: &[[[f32; 4]; 4]],
    ) -> Vec<[[f32; 4]; 4]> {
        self.joints
            .iter()
            .enumerate()
            .map(|(i, joint)| {
                let world = Matrix4::from(world_transforms[i]);
                let ibm = Matrix4::from(joint.inverse_bind_matrix);
                let joint_matrix: [[f32; 4]; 4] = (world * ibm).into();
                joint_matrix
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: identity matrix as [[f32;4];4]
    fn identity() -> [[f32; 4]; 4] {
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }

    /// Helper: translation matrix as [[f32;4];4] (column-major)
    fn translation_mat(x: f32, y: f32, z: f32) -> [[f32; 4]; 4] {
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [x, y, z, 1.0],
        ]
    }

    /// Assert two mat4s are approximately equal.
    fn assert_mat4_approx_eq(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4], eps: f32) {
        for col in 0..4 {
            for row in 0..4 {
                assert!(
                    (a[col][row] - b[col][row]).abs() < eps,
                    "mismatch at [{}][{}]: {} vs {} (eps={})",
                    col,
                    row,
                    a[col][row],
                    b[col][row],
                    eps,
                );
            }
        }
    }

    /// Simple 2-joint chain like RiggedSimple: root at origin, child offset 2.0 along Y.
    /// Inverse bind matrices are the inverse of each joint's world-space bind pose.
    fn make_two_joint_skeleton() -> Skeleton {
        // Root joint: at origin
        let root = Joint {
            index: 0,
            node_index: 0,
            name: Some("root".into()),
            parent: None,
            // Inverse of identity = identity
            inverse_bind_matrix: identity(),
            local_transform: JointTransform::Decomposed {
                translation: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0], // identity quaternion
                scale: [1.0, 1.0, 1.0],
            },
        };

        // Child joint: offset 2.0 along Y from root
        // World position in bind pose = (0, 2, 0)
        // Inverse bind matrix = inverse of translation(0, 2, 0) = translation(0, -2, 0)
        let child = Joint {
            index: 1,
            node_index: 1,
            name: Some("child".into()),
            parent: Some(0),
            inverse_bind_matrix: translation_mat(0.0, -2.0, 0.0),
            local_transform: JointTransform::Decomposed {
                translation: [0.0, 2.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
        };

        Skeleton {
            joints: vec![root, child],
        }
    }

    #[test]
    fn test_bind_pose_produces_identity_joint_matrices() {
        let skel = make_two_joint_skeleton();
        let joint_matrices = skel.compute_joint_matrices();

        assert_eq!(joint_matrices.len(), 2);

        // In bind pose, joint_matrix = world_transform * inverse_bind_matrix = identity
        // because the inverse bind matrix is exactly the inverse of the bind-pose world transform.
        let id = identity();
        assert_mat4_approx_eq(&joint_matrices[0], &id, 1e-6);
        assert_mat4_approx_eq(&joint_matrices[1], &id, 1e-6);
    }

    #[test]
    fn test_world_transforms() {
        let skel = make_two_joint_skeleton();
        let world = skel.compute_world_transforms();

        assert_eq!(world.len(), 2);

        // Root world = identity (at origin)
        assert_mat4_approx_eq(&world[0], &identity(), 1e-6);

        // Child world = translation(0, 2, 0)
        assert_mat4_approx_eq(&world[1], &translation_mat(0.0, 2.0, 0.0), 1e-6);
    }

    #[test]
    fn test_rotated_joint_produces_expected_matrix() {
        // Root at origin, child offset along Y.
        // Apply a 90-degree rotation around Z to the child joint.
        let skel = make_two_joint_skeleton();

        // 90 degrees around Z: quaternion = (0, 0, sin(45), cos(45))
        let half_angle = std::f32::consts::FRAC_PI_4; // 45 degrees = pi/4
        let sin_ha = half_angle.sin();
        let cos_ha = half_angle.cos();

        // Build posed local transforms: root stays the same, child gets rotation
        let root_local: [[f32; 4]; 4] = skel.joints[0].local_transform.to_matrix4().into();

        // For the child, we apply the rotation to its local transform:
        // local = T(0,2,0) * R(90 deg around Z) * S(1,1,1)
        let child_t = Matrix4::new_translation(&Vector3::new(0.0, 2.0, 0.0));
        let child_r = UnitQuaternion::new_normalize(Quaternion::new(cos_ha, 0.0, 0.0, sin_ha))
            .to_homogeneous();
        let child_posed: [[f32; 4]; 4] = (child_t * child_r).into();

        let local_transforms = vec![root_local, child_posed];
        let joint_matrices = skel.compute_joint_matrices_with_pose(&local_transforms);

        assert_eq!(joint_matrices.len(), 2);

        // Root joint matrix should still be identity (no change to root)
        assert_mat4_approx_eq(&joint_matrices[0], &identity(), 1e-6);

        // Child joint matrix = posed_world * inverse_bind_matrix
        // posed_world = root_world * child_local_posed = I * (T(0,2,0) * R(90Z))
        // inverse_bind = T(0,-2,0)
        // joint_matrix = T(0,2,0) * R(90Z) * T(0,-2,0)
        let expected = child_t * child_r * Matrix4::from(translation_mat(0.0, -2.0, 0.0));
        let expected_arr: [[f32; 4]; 4] = expected.into();
        assert_mat4_approx_eq(&joint_matrices[1], &expected_arr, 1e-6);
    }

    #[test]
    fn test_matrix_joint_transform() {
        // Test that JointTransform::Matrix is handled correctly
        let skel = Skeleton {
            joints: vec![Joint {
                index: 0,
                node_index: 0,
                name: None,
                parent: None,
                inverse_bind_matrix: identity(),
                local_transform: JointTransform::Matrix(translation_mat(1.0, 2.0, 3.0)),
            }],
        };

        let world = skel.compute_world_transforms();
        assert_mat4_approx_eq(&world[0], &translation_mat(1.0, 2.0, 3.0), 1e-6);
    }

    #[test]
    fn test_pose_fallback_to_rest_pose() {
        let skel = make_two_joint_skeleton();

        // Provide only one local transform (for root). Child should fall back to rest pose.
        let root_local: [[f32; 4]; 4] = skel.joints[0].local_transform.to_matrix4().into();
        let local_transforms = vec![root_local]; // only root, no child entry

        let joint_matrices = skel.compute_joint_matrices_with_pose(&local_transforms);

        // Should produce the same result as bind-pose (identity matrices)
        let id = identity();
        assert_mat4_approx_eq(&joint_matrices[0], &id, 1e-6);
        assert_mat4_approx_eq(&joint_matrices[1], &id, 1e-6);
    }
}
