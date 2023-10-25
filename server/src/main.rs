use std::net::SocketAddr;

use anyhow::Context;
use rrp::project_dir;
use tonic::transport::{Server, ServerTlsConfig};

mod config;
mod services;
mod tls;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The config is a shared immutable structure that is needed
    // throughout the entire lifetime of the app, leaking it has no downsides.
    let config: &'static _ = Box::leak(Box::new(config::Config::parse()));

    let addr = SocketAddr::new(config.ip, config.port);
    let identity = tls::load_server_identity(project_dir().config_dir())?;

    println!("Server listening on address: {}", addr);
    Server::builder()
        .tls_config(ServerTlsConfig::new().identity(identity))?
        .add_service(services::ReverseProxyService::new())
        .serve(addr)
        .await
        .context("failed to start the server")?;

    Ok(())
}
