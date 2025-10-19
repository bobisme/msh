#[cfg(feature = "renderdoc")]
use renderdoc::{RenderDoc, V140};

#[cfg(feature = "renderdoc")]
pub struct RenderDocCapture {
    rd: Option<RenderDoc<V140>>,
}

#[cfg(feature = "renderdoc")]
impl RenderDocCapture {
    pub fn new() -> Self {
        match RenderDoc::new() {
            Ok(rd) => {
                println!("✓ RenderDoc API initialized");
                Self { rd: Some(rd) }
            }
            Err(e) => {
                println!("⚠ RenderDoc not available: {}", e);
                println!("  Frame capture will not be available.");
                println!("\n  Troubleshooting:");
                println!("  1. Launch your app from RenderDoc GUI (qrenderdoc):");
                println!(
                    "     File -> Launch Application -> Select ./target/debug/msh or ./target/release/msh"
                );
                println!("  2. Or set LD_LIBRARY_PATH:");
                println!("     export LD_LIBRARY_PATH=/usr/lib:$LD_LIBRARY_PATH");
                println!(
                    "  3. Library location: {}",
                    std::env::var("LD_LIBRARY_PATH").unwrap_or_else(|_| "Not set".to_string())
                );
                Self { rd: None }
            }
        }
    }

    pub fn is_available(&self) -> bool {
        self.rd.is_some()
    }

    pub fn trigger_capture(&mut self, path_template: Option<&str>) {
        if let Some(rd) = &mut self.rd {
            // Set capture file path template if provided
            if let Some(template) = path_template {
                rd.set_capture_file_path_template(template);
                println!("📸 Frame capture triggered! Output template: {}", template);
            } else {
                println!("📸 Frame capture triggered!");
            }
            rd.trigger_capture();
        } else {
            println!("⚠ RenderDoc not available - cannot capture frame");
        }
    }
}

#[cfg(feature = "renderdoc")]
impl Default for RenderDocCapture {
    fn default() -> Self {
        Self::new()
    }
}
