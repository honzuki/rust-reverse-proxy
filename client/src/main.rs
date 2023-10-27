use rrp::setup_project_dir;

mod cli;
mod proxy;
mod server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_project_dir()?;

    cli::run().await
}
