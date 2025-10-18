#[cfg(feature = "remote")]
use jsonrpsee::server::Server;
#[cfg(feature = "remote")]
use std::net::SocketAddr;
#[cfg(feature = "remote")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "remote")]
use crate::viewer::{ViewerCommand, ViewerState};
#[cfg(feature = "remote")]
use super::methods::{ViewerRpcImpl, ViewerRpcServer};

#[cfg(feature = "remote")]
pub async fn start_rpc_server(
    state: Arc<Mutex<ViewerState>>,
    command_tx: crossbeam::channel::Sender<ViewerCommand>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;

    println!("Starting JSON-RPC server on http://{}", addr);

    let server = Server::builder()
        .build(addr)
        .await?;

    let rpc_impl = ViewerRpcImpl {
        state,
        command_tx,
    };

    let handle = server.start(rpc_impl.into_rpc());

    println!("✓ RPC server ready at http://{}", addr);
    println!("  Available methods:");
    println!("    - load_model(path, mesh_name?)");
    println!("    - set_rotation(x, y, z)");
    println!("    - rotate_around_axis(axis, angle)");
    println!("    - set_camera_position(x, y, z)");
    println!("    - set_camera_target(x, y, z)");
    println!("    - enable_wireframe/disable_wireframe/toggle_wireframe");
    println!("    - enable_backfaces/disable_backfaces/toggle_backfaces");
    println!("    - enable_ui/disable_ui/toggle_ui");
    println!("    - get_stats()");
    #[cfg(feature = "renderdoc")]
    println!("    - capture_frame(path?)");

    // Keep server running
    handle.stopped().await;

    Ok(())
}

#[cfg(feature = "remote")]
pub fn spawn_rpc_server(
    state: Arc<Mutex<ViewerState>>,
    command_tx: crossbeam::channel::Sender<ViewerCommand>,
    port: u16,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new()
            .expect("Failed to create Tokio runtime");

        rt.block_on(async {
            if let Err(e) = start_rpc_server(state, command_tx, port).await {
                eprintln!("RPC server error: {}", e);
            }
        });
    })
}
