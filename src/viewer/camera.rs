use nalgebra as na;

use super::state::ProjectionMode;

const SENSITIVITY_FACTOR: f32 = 0.0005;

/// Arc-ball camera for orbital rotation around a target point
pub struct ArcBallCamera {
    /// Camera position in world space
    eye: na::Point3<f32>,
    /// Target point (look-at)
    target: na::Point3<f32>,
    /// Up vector
    up: na::Vector3<f32>,
    /// Distance from target
    distance: f32,
    /// Vertical angle (pitch)
    theta: f32,
    /// Horizontal angle (yaw)
    phi: f32,
    /// Viewport width
    width: u32,
    /// Viewport height
    height: u32,
}

impl ArcBallCamera {
    /// Create a new arc-ball camera
    pub fn new(eye: na::Point3<f32>, target: na::Point3<f32>, width: u32, height: u32) -> Self {
        let to_eye = eye - target; // Vector FROM target TO eye
        let distance = to_eye.norm();

        // Calculate initial angles based on spherical coordinates
        // theta is vertical angle (pitch), phi is horizontal angle (yaw)
        let horizontal_dist = (to_eye.x * to_eye.x + to_eye.z * to_eye.z).sqrt();
        let theta = (-to_eye.y).atan2(horizontal_dist);
        let phi = to_eye.x.atan2(to_eye.z);

        Self {
            eye,
            target,
            up: na::Vector3::y(),
            distance,
            theta,
            phi,
            width,
            height,
        }
    }

    /// Get view matrix
    pub fn view_matrix(&self) -> na::Matrix4<f32> {
        na::Matrix4::look_at_rh(&self.eye, &self.target, &self.view_up())
    }

    /// Get projection matrix for a specific projection mode
    pub fn projection_matrix_for(&self, mode: &ProjectionMode) -> na::Matrix4<f32> {
        let aspect = self.width as f32 / self.height as f32;
        match mode {
            ProjectionMode::Perspective { fov_y_degrees } => {
                na::Matrix4::new_perspective(aspect, fov_y_degrees.to_radians(), 0.1, 1000.0)
            }
            ProjectionMode::Orthographic { world_height } => {
                let half_h = world_height / 2.0;
                let half_w = half_h * aspect;
                // nalgebra's new_orthographic maps z to [-1,1] (OpenGL convention).
                // wgpu uses [0,1] depth (Vulkan/Metal/DX12), so remap: z' = z * 0.5 + 0.5
                let mut proj =
                    na::Matrix4::new_orthographic(-half_w, half_w, -half_h, half_h, 0.1, 1000.0);
                // Apply z remapping: scale z by 0.5 and offset by 0.5
                proj[(2, 2)] *= 0.5;
                proj[(2, 3)] = proj[(2, 3)] * 0.5 + 0.5;
                proj
            }
        }
    }

    /// Get combined view-projection matrix for a specific projection mode
    pub fn view_projection_matrix_for(&self, mode: &ProjectionMode) -> na::Matrix4<f32> {
        self.projection_matrix_for(mode) * self.view_matrix()
    }

    /// Handle mouse drag for rotation
    pub fn rotate(&mut self, delta_x: f32, delta_y: f32) {
        let sensitivity = 0.005;
        self.phi -= delta_x * sensitivity; // Negative so dragging right rotates model right
        self.theta = (self.theta - delta_y * sensitivity).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );

        self.update_position();
    }

    /// Handle mouse drag for panning
    pub fn pan(&mut self, delta_x: f32, delta_y: f32) {
        let sensitivity = SENSITIVITY_FACTOR * self.distance;

        // Calculate right and up vectors in camera space
        let forward = (self.target - self.eye).normalize();
        let right = forward.cross(&self.view_up()).normalize();
        let up = right.cross(&forward);

        // Move target and eye together
        let offset = right * (-delta_x * sensitivity) + up * (delta_y * sensitivity);
        self.target += offset;
        self.eye += offset;
    }

    /// Handle scroll for zoom
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (1.0 - delta * 0.1)).max(0.1);
        self.update_position();
    }

    /// Update camera position based on spherical coordinates
    fn update_position(&mut self) {
        self.eye.x = self.target.x + self.distance * self.theta.cos() * self.phi.sin();
        self.eye.y = self.target.y - self.distance * self.theta.sin();
        self.eye.z = self.target.z + self.distance * self.theta.cos() * self.phi.cos();
    }

    /// Update a complete pose without exposing an intermediate invalid look-at.
    pub fn set_pose(
        &mut self,
        eye: Option<[f32; 3]>,
        target: Option<[f32; 3]>,
        up: Option<[f32; 3]>,
    ) -> Result<(), String> {
        let eye = eye.map(na::Point3::from).unwrap_or(self.eye);
        let target = target.map(na::Point3::from).unwrap_or(self.target);
        let up = up.map(na::Vector3::from).unwrap_or(self.up);
        if !eye
            .iter()
            .chain(target.iter())
            .chain(up.iter())
            .all(|v| v.is_finite())
        {
            return Err("Camera coordinates must be finite".into());
        }
        let to_eye = eye - target;
        if to_eye.norm_squared() < 1e-12 || up.norm_squared() < 1e-12 {
            return Err("Camera eye must differ from target and up must be nonzero".into());
        }
        self.eye = eye;
        self.target = target;
        self.up = up.normalize();
        self.distance = to_eye.norm();
        let horizontal = to_eye.x.hypot(to_eye.z);
        self.theta = (-to_eye.y).atan2(horizontal);
        self.phi = to_eye.x.atan2(to_eye.z);
        Ok(())
    }

    // A camera at a pole needs an up vector orthogonal to the viewing direction.
    fn view_up(&self) -> na::Vector3<f32> {
        let forward = (self.target - self.eye).normalize();
        if forward.cross(&self.up).norm_squared() < 1e-8 {
            if forward.z.abs() < 0.9 {
                na::Vector3::z()
            } else {
                na::Vector3::x()
            }
        } else {
            self.up
        }
    }

    /// Get current position
    pub fn position(&self) -> na::Point3<f32> {
        self.eye
    }

    /// Update viewport size
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_pose_preserves_orbit_and_pole_views_are_finite() {
        let mut camera = ArcBallCamera::new(
            na::Point3::new(3.0, 2.0, 4.0),
            na::Point3::origin(),
            800,
            600,
        );
        camera
            .set_pose(Some([4.0, 6.0, -2.0]), Some([1.0, 2.0, 1.0]), None)
            .unwrap();
        let eye = camera.position();
        camera.rotate(0.0, 0.0);
        assert!((camera.position() - eye).norm() < 1e-5);
        for y in [-5.0, 5.0] {
            camera
                .set_pose(Some([1.0, y, 1.0]), Some([1.0, 0.0, 1.0]), None)
                .unwrap();
            assert!(camera.view_matrix().iter().all(|v| v.is_finite()));
            camera.pan(2.0, 3.0);
            assert!(camera.view_matrix().iter().all(|v| v.is_finite()));
        }
        let eye = camera.position();
        assert!(
            camera
                .set_pose(Some([f32::NAN, 0.0, 0.0]), None, None)
                .is_err()
        );
        assert_eq!(camera.position(), eye);
    }
}
