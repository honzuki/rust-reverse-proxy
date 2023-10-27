use std::collections::HashMap;

use anyhow::Context;
use rrp::{
    auth::{generate_token, hash_token, TokenHash},
    project_dir,
};
use serde::{Deserialize, Serialize};
use tokio::{fs, io::AsyncWriteExt};
use tonic::{service::interceptor::InterceptedService, transport::Certificate};

const SERVER_LIST_FILE_NAME: &str = "servers.toml";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerList {
    #[serde(flatten)]
    list: HashMap<String, Server>,
}

impl ServerList {
    // Adds a new server to the server list
    //
    // Will overwrite an already existing server with the same identifier if exists.
    // Saves the new server list to the disk before returning.
    pub async fn add_server(
        &mut self,
        identifier: String,
        url: String,
        certificate: Vec<u8>,
        certificate_hostname: Option<String>,
    ) -> tokio::io::Result<()> {
        // Any of the alt certificate hostnames is good for default
        let certificate_hostname =
            certificate_hostname.unwrap_or_else(|| rrp::tls::DEFAULT_ALT_NAMES[0].into());

        // Add the server to the list
        // overwrite an existing server if necessary
        let server = Server {
            url,
            certificate,
            certificate_hostname,
            token: generate_token(),
        };
        self.list.insert(identifier, server);

        // Save the new list to disk
        let server_list_path = project_dir().config_dir().join(SERVER_LIST_FILE_NAME);
        let mut file = fs::File::create(server_list_path).await?;
        file.write_all(
            toml::to_string(&self)
                .expect("serialize the server list into toml")
                .as_bytes(),
        )
        .await?;

        Ok(())
    }

    pub async fn load_from_disk() -> anyhow::Result<Self> {
        let server_list_path = project_dir().config_dir().join(SERVER_LIST_FILE_NAME);

        // create a new file if necessary
        if !server_list_path.exists() {
            fs::File::create(server_list_path)
                .await
                .context("failed to create the config file")?;

            return Ok(Self {
                list: HashMap::default(),
            });
        }

        let list: Self = toml::from_str(
            &fs::read_to_string(server_list_path)
                .await
                .context("failed to read the server list file")?,
        )
        .context("failed to parse the server list file")?;

        Ok(list)
    }

    pub fn get_server(&self, identifier: &str) -> Option<&Server> {
        self.list.get(identifier)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Server {
    url: String,
    certificate_hostname: String,
    certificate: Vec<u8>,
    token: String,
}

impl Server {
    pub async fn open_grpc_channel(
        &self,
    ) -> anyhow::Result<
        InterceptedService<
            tonic::transport::Channel,
            impl Fn(tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> + Clone,
        >,
    > {
        let token: tonic::metadata::MetadataValue<_> = self.token.clone().parse()?;
        let attach_auth_middleware = move |mut request: tonic::Request<()>| {
            request
                .metadata_mut()
                .insert(rrp::auth::METADATA_TOKEN, token.clone());
            Ok(request)
        };

        let tls = tonic::transport::ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(&self.certificate))
            .domain_name(self.certificate_hostname.clone());

        let channel = tonic::transport::Channel::from_shared(self.url.to_string())
            .context("Failed to parse the server details")?
            .tls_config(tls)
            .context("Failed to parse the server certificate")?
            .connect()
            .await?;

        // attach authentication token to all requests
        Ok(InterceptedService::new(channel, attach_auth_middleware))
    }

    pub fn hashed_token(&self) -> anyhow::Result<TokenHash> {
        hash_token(&self.token).context("failed to hash the token")
    }
}
