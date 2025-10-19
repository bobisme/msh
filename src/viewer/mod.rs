pub mod render;
pub mod state;

#[cfg(feature = "remote")]
pub mod rpc_viewer;

#[cfg(feature = "renderdoc")]
pub mod renderdoc_helper;

pub use render::view_mesh;
pub use state::{MeshStats, ViewerCommand, ViewerState};

#[cfg(feature = "remote")]
pub use rpc_viewer::view_mesh_with_rpc;
