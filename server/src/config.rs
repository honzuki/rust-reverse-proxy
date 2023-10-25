use std::{io::Write, net::IpAddr};

use anyhow::Context;
use clap::Parser;
use rrp::project_dir;
use serde::{Deserialize, Serialize};

const SERVER_CONFIG_FILE_NAME: &str = "server.toml";

// The external interface
pub struct Config {
    pub ip: IpAddr,
    pub port: u16,
}

impl Config {
    pub fn parse() -> Config {
        let cli = CliArgs::parse();
        let file = ConfigFile::parse_config().unwrap_or_else(|err| {
            eprintln!("{}", err);
            ConfigFile::default()
        });

        // combine both config options into one final structure
        Config {
            ip: cli.ip.unwrap_or(file.ip),
            port: cli.port.unwrap_or(file.port),
        }
    }
}

// Default values
fn default_ip() -> IpAddr {
    "0.0.0.0".parse().unwrap()
}

fn default_port() -> u16 {
    3600
}

// Config file
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ConfigFile {
    #[serde(default = "default_ip")]
    ip: IpAddr,

    #[serde(default = "default_port")]
    port: u16,
}

impl Default for ConfigFile {
    fn default() -> Self {
        toml::from_str("").unwrap()
    }
}

impl ConfigFile {
    // Tries to parse the config file from the default project config directory.
    //
    // if a config file is not present it will generate a new file using the default values.
    fn parse_config() -> anyhow::Result<ConfigFile> {
        let config_dir = project_dir().config_dir();
        let config_path = config_dir.join(SERVER_CONFIG_FILE_NAME);

        if !config_path.exists() {
            let default_config = ConfigFile::default();
            let cfg_data = toml::to_string_pretty(&default_config).unwrap();

            let mut file =
                std::fs::File::create(config_path).context("failed to create the config file")?;

            file.write_all(cfg_data.as_bytes())
                .context("failed to write the default config file")?;

            return Ok(default_config);
        }

        // try to read & parse the data from the config file
        let cfg_data =
            std::fs::read_to_string(config_path).context("failed to read the config file")?;

        toml::from_str(&cfg_data).context("failed to parse the config file")
    }
}

// CLI args can be used to override the config file
#[derive(Debug, Parser)]
#[command(version)]
struct CliArgs {
    /// Network ip to use
    #[arg(short, long)]
    ip: Option<IpAddr>,

    /// Network port to use
    #[arg(short, long)]
    port: Option<u16>,
}
