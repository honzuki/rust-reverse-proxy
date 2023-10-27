use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use tokio::fs;

use crate::{proxy, server::ServerList};

#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    // Add a new server
    Add {
        /// A user-provided identifier that is used to identify
        /// this server with other commands
        ///
        /// The identifier must be unique, using an existing
        /// identifier will overwrite the existing one.
        #[arg(short, long)]
        identifier: String,

        ///  The server address
        #[arg(long, value_name = "https://[ip/domain]:port")]
        url: String,

        /// The path for the server tls certificate, in pem format
        #[arg(short, long, value_name = "path/to/server.pem")]
        certificate: PathBuf,

        /// The certificate hostname
        ///
        /// you should not put an hostname for auto-generated self-signed certificates
        #[arg(long)]
        certificate_hostname: Option<String>,
    },

    // Expose an internal port through the server
    Expose {
        /// The server's identifier through which you
        /// want to expose the port
        #[arg(short, long)]
        server: String,

        /// The protocol
        #[arg(value_enum, short, long)]
        protocol: Protocol,

        /// The local port you want to expose
        #[arg(short, long)]
        local: u16,

        /// The external port you want to use,
        /// if you don't provide this field the server
        /// will use a random open port
        #[arg(short, long)]
        external: Option<u16>,
    },
}

#[derive(ValueEnum, Clone)]
pub enum Protocol {
    Tcp,
}

pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut servers = ServerList::load_from_disk().await?;

    match cli.command {
        Commands::Add {
            identifier,
            url,
            certificate,
            certificate_hostname,
        } => {
            let certificate = fs::read_to_string(certificate)
                .await
                .context("failed to read the server's certificate!")?;

            servers
                .add_server(
                    identifier.clone(),
                    url,
                    certificate.into_bytes().to_vec(),
                    certificate_hostname,
                )
                .await
                .context("failed to update the server list")?;

            let server = servers.get_server(&identifier).unwrap();
            println!(
                "\"{}\" was added successfully. \n\nThe generated hashed client token is:\n\"{}\"",
                identifier,
                server.hashed_token()?
            );
        }
        Commands::Expose {
            server,
            protocol,
            local,
            external,
        } => {
            let server = servers.get_server(&server).with_context(|| {
                format!("can not find a server with \"{}\" as identifier", server)
            })?;

            match protocol {
                Protocol::Tcp => proxy::tcp::expose_port(server, local, external).await?,
            }
        }
    };

    Ok(())
}
