use jsonrpsee::server::{Server, ServerConfig, ServerHandle};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};

use super::methods::{ViewerRpcImpl, ViewerRpcServer};
use crate::viewer::{ViewerCommand, ViewerState};

/// The listener is bound before startup returns, so port 0 and bind errors are real.
pub fn spawn_rpc_server(
    state: Arc<Mutex<ViewerState>>,
    command_tx: crossbeam::channel::Sender<ViewerCommand>,
    port: u16,
) -> Result<(SocketAddr, ServerHandle), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(async {
                let server = Server::builder()
                    .set_config(
                        ServerConfig::builder()
                            .max_request_body_size(1024 * 1024)
                            .max_connections(64)
                            .build(),
                    )
                    .build_from_tcp(listener)?;
                let implementation = ViewerRpcImpl { state, command_tx };
                let handle = server.start(implementation.into_rpc());
                let _ = ready_tx.send(Ok(handle.clone()));
                handle.stopped().await;
                Ok(())
            })
        })();
        if let Err(error) = result {
            let _ = ready_tx.send(Err(error.to_string()));
        }
    });
    let handle = ready_rx.recv().map_err(|e| e.to_string())??;
    println!("RPC_READY http://{address}");
    Ok((address, handle))
}
