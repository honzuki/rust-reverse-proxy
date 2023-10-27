use std::net::SocketAddr;

use anyhow::Context;
use rrp::{project_dir, setup_project_dir};
use tonic::transport::{Server, ServerTlsConfig};

mod auth;
mod config;
mod services;
mod tls;
mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_project_dir().context("failed to setup project directories!")?;

    // These are shared immutable structures that are needed
    // throughout the entire lifetime of the app, leaking them has no downsides.
    let config: &'static _ = Box::leak(Box::new(config::Config::parse()));
    let shared_auth: &'static _ = Box::leak(Box::new(auth::Auth::load_from_file(
        project_dir().config_dir(),
    )?));

    let addr = SocketAddr::new(config.ip, config.port);
    let identity = tls::load_server_identity(project_dir().config_dir())?;

    println!("Server listening on address: {}", addr);
    Server::builder()
        .tls_config(ServerTlsConfig::new().identity(identity))?
        .add_service(auth::attach_auth(
            shared_auth,
            services::ReverseProxyService::new(),
        ))
        .serve(addr)
        .await
        .context("failed to start the server")?;

    Ok(())
}
