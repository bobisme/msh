pub mod camera;
pub mod gpu;
pub mod mesh_renderer;
pub mod render;
pub mod state;
pub mod ui_renderer;
pub mod bvh_viewer;

mod shaders {
    // Empty module to include shader directory
}

#[cfg(feature = "remote")]
pub mod rpc_viewer;

#[cfg(feature = "renderdoc")]
pub mod renderdoc_helper;

pub mod headless;
pub mod sprite_sheet;

pub use render::view_mesh_with_bvh;
pub use bvh_viewer::view_bvh;
pub use state::{ViewerCommand, ViewerState};

#[cfg(feature = "remote")]
pub use rpc_viewer::view_mesh_with_rpc;
