use nalgebra as na;

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
        na::Matrix4::look_at_rh(&self.eye, &self.target, &self.up)
    }

    /// Get projection matrix
    pub fn projection_matrix(&self) -> na::Matrix4<f32> {
        let aspect = self.width as f32 / self.height as f32;
        na::Matrix4::new_perspective(aspect, 45.0_f32.to_radians(), 0.1, 1000.0)
    }

    /// Get combined view-projection matrix
    pub fn view_projection_matrix(&self) -> na::Matrix4<f32> {
        self.projection_matrix() * self.view_matrix()
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
        let right = forward.cross(&self.up).normalize();
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

    /// Set camera position
    pub fn set_position(&mut self, position: na::Point3<f32>) {
        self.eye = position;
        let to_target = self.target - self.eye;
        self.distance = to_target.norm();

        // Recalculate angles
        let horizontal_dist = (to_target.x * to_target.x + to_target.z * to_target.z).sqrt();
        self.theta = (-to_target.y).atan2(horizontal_dist);
        self.phi = to_target.x.atan2(to_target.z);
    }

    /// Set camera target
    pub fn set_target(&mut self, target: na::Point3<f32>) {
        self.target = target;
        let to_target = self.target - self.eye;
        self.distance = to_target.norm();

        // Recalculate angles
        let horizontal_dist = (to_target.x * to_target.x + to_target.z * to_target.z).sqrt();
        self.theta = (-to_target.y).atan2(horizontal_dist);
        self.phi = to_target.x.atan2(to_target.z);
    }

    /// Get current position
    pub fn position(&self) -> na::Point3<f32> {
        self.eye
    }

    /// Get current target
    pub fn target(&self) -> na::Point3<f32> {
        self.target
    }

    /// Update viewport size
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }
}
