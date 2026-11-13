//! Pre-filtered linear environments for metallic-roughness image-based lighting.
use nalgebra::Vector3;
use std::{f32::consts::PI, path::Path, sync::OnceLock};

type V = Vector3<f32>;
const WIDTH: u32 = 128;
const HEIGHT: u32 = 64;
const LEVELS: usize = 8;
const SAMPLES: usize = 128;

#[derive(Debug)]
pub struct EnvironmentLevel {
    pub width: u32,
    pub height: u32,
    /// Tightly packed, top-to-bottom linear RGBA16F pixels.
    pub pixels: Vec<u16>,
}
#[derive(Debug)]
pub struct EnvironmentMap {
    pub specular: Vec<EnvironmentLevel>,
    pub diffuse: EnvironmentLevel,
}

impl EnvironmentMap {
    pub fn studio() -> &'static Self {
        static STUDIO: OnceLock<EnvironmentMap> = OnceLock::new();
        STUDIO.get_or_init(|| Self::filter(studio_radiance))
    }

    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let extension = path.extension().and_then(|x| x.to_str()).unwrap_or("");
        if !extension.eq_ignore_ascii_case("hdr") && !extension.eq_ignore_ascii_case("exr") {
            return Err("Environment must be a linear equirectangular .hdr or .exr image".into());
        }
        let (width, height) = image::image_dimensions(path)?;
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || u64::from(width) * u64::from(height) > 16_777_216
        {
            return Err(
                "Environment image exceeds the 8192-pixel axis or 16-megapixel limit".into(),
            );
        }
        let image = image::open(path)?.to_rgb32f();
        if image.pixels().any(|p| p.0.iter().any(|v| !v.is_finite())) {
            return Err("Environment image contains nonfinite radiance".into());
        }
        Ok(Self::filter(|direction| {
            let u = (direction.x.atan2(direction.z) / (2.0 * PI) + 0.5).rem_euclid(1.0);
            let v = direction.y.clamp(-1.0, 1.0).acos() / PI;
            let x = u * width as f32 - 0.5;
            let y = v * height as f32 - 0.5;
            let x0 = x.floor() as i64;
            let y0 = y.floor() as i64;
            let tx = x - x.floor();
            let ty = y - y.floor();
            let get = |x: i64, y: i64| {
                let p = image
                    .get_pixel(
                        x.rem_euclid(width as i64) as u32,
                        y.clamp(0, height as i64 - 1) as u32,
                    )
                    .0;
                V::new(p[0].max(0.0), p[1].max(0.0), p[2].max(0.0))
            };
            let a = get(x0, y0) * (1.0 - tx) + get(x0 + 1, y0) * tx;
            let b = get(x0, y0 + 1) * (1.0 - tx) + get(x0 + 1, y0 + 1) * tx;
            a * (1.0 - ty) + b * ty
        }))
    }

    fn filter(sample: impl Fn(V) -> V) -> Self {
        let mut specular = Vec::with_capacity(LEVELS);
        for level in 0..LEVELS {
            let width = (WIDTH >> level).max(1);
            let height = (HEIGHT >> level).max(1);
            let roughness = level as f32 / (LEVELS - 1) as f32;
            let alpha = roughness * roughness;
            let a2 = alpha * alpha;
            let samples: Vec<(V, f32)> = (0..SAMPLES)
                .filter_map(|i| {
                    let u = (i as f32 + 0.5) / SAMPLES as f32;
                    let phi = 2.0 * PI * ((i as f32 * 0.61803398875).fract());
                    let cos_theta = ((1.0 - u) / (1.0 + (a2 - 1.0) * u)).sqrt();
                    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
                    let h = V::new(sin_theta * phi.cos(), sin_theta * phi.sin(), cos_theta);
                    let l = h * (2.0 * cos_theta) - V::z();
                    (l.z > 0.0).then_some((l, l.z))
                })
                .collect();
            specular.push(make_level(width, height, |n| {
                if level == 0 {
                    return sample(n);
                }
                let (t, b) = basis(n);
                let mut sum = V::zeros();
                let mut weight = 0.0;
                for (l, w) in &samples {
                    sum += sample(t * l.x + b * l.y + n * l.z) * *w;
                    weight += *w;
                }
                sum / weight.max(1e-6)
            }));
        }
        let samples: Vec<V> = (0..SAMPLES)
            .map(|i| {
                let u = (i as f32 + 0.5) / SAMPLES as f32;
                let phi = 2.0 * PI * ((i as f32 * 0.61803398875).fract());
                V::new(u.sqrt() * phi.cos(), u.sqrt() * phi.sin(), (1.0 - u).sqrt())
            })
            .collect();
        let diffuse = make_level(32, 16, |n| {
            let (t, b) = basis(n);
            samples.iter().fold(V::zeros(), |sum, l| {
                sum + sample(t * l.x + b * l.y + n * l.z)
            }) / SAMPLES as f32
        });
        Self { specular, diffuse }
    }
}

fn basis(n: V) -> (V, V) {
    let up = if n.y.abs() > 0.99 { V::x() } else { V::y() };
    let t = up.cross(&n).normalize();
    (t, n.cross(&t))
}

fn make_level(width: u32, height: u32, sample: impl Fn(V) -> V) -> EnvironmentLevel {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        let theta = PI * (y as f32 + 0.5) / height as f32;
        for x in 0..width {
            let phi = 2.0 * PI * ((x as f32 + 0.5) / width as f32 - 0.5);
            let n = V::new(
                theta.sin() * phi.sin(),
                theta.cos(),
                theta.sin() * phi.cos(),
            );
            let rgb = sample(n);
            for value in rgb.iter() {
                pixels.push(half::f16::from_f32(value.clamp(0.0, 65504.0)).to_bits());
            }
            pixels.push(half::f16::ONE.to_bits());
        }
    }
    EnvironmentLevel {
        width,
        height,
        pixels,
    }
}

fn studio_radiance(dir: V) -> V {
    let t = (dir.y * 0.5 + 0.5).clamp(0.0, 1.0);
    let sky = V::new(0.06, 0.055, 0.05) * (1.0 - t) + V::new(0.35, 0.38, 0.43) * t;
    let panel1 = dir
        .dot(&V::new(-0.6f32, 0.7, 0.4).normalize())
        .max(0.0)
        .powi(16);
    let panel2 = dir
        .dot(&V::new(0.7f32, 0.45, -0.55).normalize())
        .max(0.0)
        .powi(32);
    sky + V::new(2.8, 2.7, 2.5) * panel1 + V::new(1.2, 1.3, 1.5) * panel2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_radiance_is_preserved_by_all_filters() {
        let environment = EnvironmentMap::filter(|_| V::new(2.0, 0.5, 0.125));
        for level in environment
            .specular
            .iter()
            .chain(std::iter::once(&environment.diffuse))
        {
            for pixel in level.pixels.chunks_exact(4) {
                for (channel, expected) in [2.0, 0.5, 0.125].into_iter().enumerate() {
                    assert!(
                        (half::f16::from_bits(pixel[channel]).to_f32() - expected).abs() < 0.002
                    );
                }
            }
        }
    }

    #[test]
    fn rejects_non_hdr_and_missing_files() {
        assert!(EnvironmentMap::load(Path::new("missing.png")).is_err());
        assert!(EnvironmentMap::load(Path::new("missing.hdr")).is_err());
    }

    #[test]
    fn prefiltered_environment_preserves_direction() {
        let environment = EnvironmentMap::filter(|direction| {
            if direction.x > 0.0 {
                V::new(4.0, 0.0, 0.0)
            } else {
                V::new(0.0, 0.0, 4.0)
            }
        });
        let level = &environment.specular[0];
        let pixel = |x: u32| {
            let offset = ((level.height / 2 * level.width + x) * 4) as usize;
            [
                half::f16::from_bits(level.pixels[offset]).to_f32(),
                half::f16::from_bits(level.pixels[offset + 2]).to_f32(),
            ]
        };
        assert_eq!(pixel(level.width * 3 / 4), [4.0, 0.0]);
        assert_eq!(pixel(level.width / 4), [0.0, 4.0]);
    }
}
