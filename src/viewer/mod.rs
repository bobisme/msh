pub mod bvh_viewer;
pub mod camera;
pub mod gpu;
mod ground;
pub mod mesh_renderer;
mod pose;
mod readback;
pub mod render;
#[cfg(feature = "remote")]
pub mod serve;
mod shadow;
pub mod state;
pub mod ui_renderer;

mod shaders {
    // Empty module to include shader directory
}

#[cfg(feature = "remote")]
pub mod rpc_viewer;

#[cfg(feature = "renderdoc")]
pub mod renderdoc_helper;

pub mod headless;
pub mod sprite_sheet;

pub use bvh_viewer::view_bvh;
pub use render::view_mesh_with_bvh;
#[cfg(feature = "remote")]
pub use state::ViewerCommand;
pub use state::ViewerState;

#[cfg(feature = "remote")]
pub use rpc_viewer::view_mesh_with_rpc;
