pub mod tcp {
    use std::net::Ipv4Addr;

    use crate::server::Server;
    use anyhow::Context;
    use rrp::grpc::{
        reverse_proxy_client::ReverseProxyClient, tcp_accept_request, tcp_bind_response, Packet,
        TcpAcceptRequest, TcpAcceptRequestMetadata, TcpBindRequest,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
    };

    // The amount of packet's from the local server to the proxy
    // that we'll buffer blocking the local server
    const LOCAL_SERVER_PACKET_BACK_PRESSURE: usize = 10;

    pub async fn expose_port(
        server: &Server,
        local_port: u16,
        external_port: Option<u16>,
    ) -> anyhow::Result<()> {
        let mut client = ReverseProxyClient::new(server.open_grpc_channel().await?);

        let mut connections_stream = client
            .bind_tcp(TcpBindRequest {
                port: external_port.map(|port| port as i32),
            })
            .await
            .context("failed to expose the local port!")?
            .into_inner();

        let metadata = connections_stream
            .message()
            .await?
            .and_then(|md| md.response)
            .and_then(|md| match md {
                tcp_bind_response::Response::Metadata(md) => Some(md),
                _ => None,
            })
            .expect("the first message from the server should always contain metadata");

        let external_port: u16 = metadata.port.try_into().unwrap();
        println!("Reverse proxy listening on port: {}", external_port);
        // we can trust the server to return a valid port number

        while let Some(message) = connections_stream.message().await? {
            if matches!(
                message.response,
                Some(tcp_bind_response::Response::Connection(_))
            ) {
                // The proxy received a new connection, we need to accept it on the client side
                let server = server.clone();
                tokio::spawn(async move {
                    if let Err(reason) =
                        accept_connection(server.clone(), local_port, external_port).await
                    {
                        eprintln!("A client connection was terminated: {}", reason);
                    }
                });
            }
        }

        Ok(())
    }

    async fn accept_connection(
        server: Server,
        local_port: u16,
        external_port: u16,
    ) -> anyhow::Result<()> {
        // open a seperate, new, connection to the proxy
        let mut client = ReverseProxyClient::new(server.open_grpc_channel().await?);
        // open a new connection to the local server
        let mut local_server = TcpStream::connect((Ipv4Addr::LOCALHOST, local_port))
            .await
            .with_context(|| format!("failed to connect to the local server at: {}", local_port))?;
        let (mut reader, mut writer) = local_server.split();

        // a channel for the messages we send to the client through the proxy
        let (rx, mut tx) = tokio::sync::mpsc::channel::<Vec<u8>>(LOCAL_SERVER_PACKET_BACK_PRESSURE);

        // create a stream from the local server's output
        let local_server_stream = async_stream::stream! {
            // we need to provide the proxy with the external port we're accepting from
            yield TcpAcceptRequest {
                request: Some(tcp_accept_request::Request::Metadata(
                    TcpAcceptRequestMetadata {
                        port: external_port as i32,
                    },
                )),
            };

            while let Some(packet) = tx.recv().await {
                yield TcpAcceptRequest {
                    request: Some(tcp_accept_request::Request::Packet(Packet { data: packet })),
                }
            }
        };

        // a future that reads from the local server and feeds the stream
        let from_local_server = async move {
            let mut data = vec![0u8; 4096];
            loop {
                let rcount = reader.read(&mut data).await?;
                if rcount == 0 {
                    break;
                }

                rx.send(data[..rcount].to_vec()).await.unwrap();
            }

            Ok::<_, anyhow::Error>(())
        };

        // accept and connect a new client to the local server through the reverse proxy
        let mut client_stream = client
            .accept_tcp_connection(local_server_stream)
            .await?
            .into_inner();

        let from_client = async move {
            while let Some(packet) = client_stream.message().await? {
                if packet.data.is_empty() {
                    writer.shutdown().await?;
                    break;
                }
                writer
                    .write_all(&packet.data)
                    .await
                    .context("failed to write to the local server socket")?;
            }

            Ok::<_, anyhow::Error>(())
        };

        // Run both futures on the same
        let (r1, r2) = tokio::join!(from_local_server, from_client);
        r1?;
        r2?;

        Ok(())
    }
}
