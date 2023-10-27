use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    path::Path,
};

use anyhow::Context;
use dashmap::DashMap;
use rrp::auth::{hash_token, TokenHash, METADATA_TOKEN};
use serde::{Deserialize, Serialize};
use tonic::{service::interceptor::InterceptedService, Request, Status};

const CLIENTS_FILE_NAME: &str = "clients.toml";
const TEMPLATE_CLIENTS_FILE_NAME: &str = "clients.toml.example";

// A simple auth middleware that saves
// allowed hashed keys that can access the server
#[derive(Debug, Default)]
pub struct Auth {
    // Map the client's hash to an identifier
    client: DashMap<TokenHash, Client>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    #[serde(skip_deserializing)]
    identifier: String,
    hashed_token: String,
}

impl Auth {
    //  Load the auth data from the client file
    //
    // if no client fils is present it will return an empty object
    pub fn load_from_file(base: &Path) -> anyhow::Result<Auth> {
        let Ok(mut client_file) = File::open(base.join(CLIENTS_FILE_NAME)) else {
            // (try to) generate a stem file to help the user
            let _ = std::fs::create_dir_all(base);
            if let Ok(mut file) = File::create(base.join(TEMPLATE_CLIENTS_FILE_NAME)) {
                let mock_data = toml::toml! {
                    [A_unique_client_identifier]
                    hashed_token = "<An hex encoded hashed version of the client's token>"
                };
                let _ = file.write_all(toml::to_string_pretty(&mock_data).unwrap().as_bytes());
            }

            eprintln!("The clients file is missing, server will reject all requests");
            return Ok(Auth::default());
        };

        let mut data = String::new();
        client_file
            .read_to_string(&mut data)
            .context("failed to read the clients file")?;
        let clients: HashMap<String, Client> =
            toml::from_str(&data).context("failed to parse the clients file")?;

        Ok(Auth {
            client: clients
                .into_iter()
                // swap the map from identifier->client to hashed_token->client
                .map(|(identifier, mut client)| {
                    // inject identifier into the struct
                    client.identifier = identifier;
                    (client.hashed_token.clone(), client)
                })
                .collect(),
        })
    }

    // Authenticate a client by token
    //
    // returns the client's info if recognized, otherwise None
    pub fn by_token(&self, token: &str) -> Option<Client> {
        let hashed_token = hash_token(token).ok()?;
        self.client.get(&hashed_token).as_deref().cloned()
    }
}

// Attach an authentication middleware to a service
pub fn attach_auth<S>(
    shared_auth: &'static Auth,
    service: S,
) -> InterceptedService<S, impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone> {
    let middleware = move |mut request: Request<()>| {
        // fetch token from request & authenticate
        let client = request
            .metadata()
            .get(METADATA_TOKEN)
            .and_then(|token| token.to_str().ok())
            .and_then(|token| shared_auth.by_token(token));

        match client {
            Some(client) => {
                // inject the client's info into the request
                request.extensions_mut().insert(client);
                Ok(request)
            }
            _ => Err(Status::unauthenticated("No valid auth token was provided")),
        }
    };

    InterceptedService::new(service, middleware)
}
