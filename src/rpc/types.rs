use serde::{Deserialize, Serialize};

/// Parse angle string like "90d" (degrees) or "1.57r" (radians)
pub fn parse_angle(s: &str) -> Result<f32, String> {
    let s = s.trim();

    if s.is_empty() {
        return Err("Empty angle string".to_string());
    }

    // Check last character for unit
    let last_char = s.chars().last().unwrap();

    match last_char {
        'd' | 'D' => {
            // Degrees
            let num_part = &s[..s.len() - 1];
            let degrees: f32 = num_part
                .parse()
                .map_err(|_| format!("Invalid number in angle: {}", num_part))?;
            Ok(degrees.to_radians())
        }
        'r' | 'R' => {
            // Radians
            let num_part = &s[..s.len() - 1];
            num_part
                .parse()
                .map_err(|_| format!("Invalid number in angle: {}", num_part))
        }
        _ => {
            // Try parsing as radians without unit
            s.parse().map_err(|_| {
                format!(
                    "Invalid angle format '{}'. Use '90d' for degrees or '1.57r' for radians",
                    s
                )
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshStatsResponse {
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
    pub triangles: usize,
    pub is_manifold: bool,
    pub holes: usize,
    pub topology: Option<crate::mesh::topology::TopologyReport>,
}

/// Successful captures are acknowledged only after their PNG has been written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotResponse {
    pub path: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shot {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub path: String,
    #[serde(default)]
    pub up: Option<[f32; 3]>,
    #[serde(default)]
    pub fov: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShotResult {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialPixels {
    pub id: u32,
    pub name: Option<String>,
    pub pixels: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixelStats {
    pub width: u32,
    pub height: u32,
    pub total_px: usize,
    pub covered_px: usize,
    pub backface_px: usize,
    /// Inclusive bounds of visible backface pixels: min x/y, max x/y.
    pub bbox: Option<[u32; 4]>,
    pub material_px: std::collections::BTreeMap<String, usize>,
    pub materials: Vec<MaterialPixels>,
    pub blend_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationResponse {
    pub name: Option<String>,
    pub index: usize,
    pub duration: f32,
    pub time: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_angle_degrees() {
        assert!((parse_angle("90d").unwrap() - std::f32::consts::FRAC_PI_2).abs() < 0.001);
        assert!((parse_angle("180D").unwrap() - std::f32::consts::PI).abs() < 0.001);
        assert!((parse_angle("45d").unwrap() - std::f32::consts::FRAC_PI_4).abs() < 0.001);
    }

    #[test]
    fn test_parse_angle_radians() {
        assert!((parse_angle("1.57r").unwrap() - 1.57).abs() < 0.001);
        assert!((parse_angle("3.14R").unwrap() - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_parse_angle_no_unit() {
        assert!((parse_angle("1.57").unwrap() - 1.57).abs() < 0.001);
        assert!((parse_angle("3.14").unwrap() - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_parse_angle_errors() {
        assert!(parse_angle("").is_err());
        assert!(parse_angle("abcd").is_err());
        assert!(parse_angle("90x").is_err());
    }
}
