use std::{future::Future, pin::Pin, task::Poll};

use dashmap::DashMap;
use rrp::grpc::{
    reverse_proxy_server::{ReverseProxy, ReverseProxyServer},
    tcp_accept_request, tcp_bind_response, Packet, TcpAcceptRequest, TcpBindRequest,
    TcpBindResponse, TcpBindResponseMetadata, TcpNewConnection,
};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    select,
};
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

use crate::utils::{self, parse_port};

// Maps a port -> TcpStream
type ConnectionQueue = DashMap<u16, Vec<TcpStream>>;

pub struct ReverseProxyService {
    pending_connections: &'static ConnectionQueue,
}

impl ReverseProxyService {
    pub fn new() -> ReverseProxyServer<Self> {
        // The service is used throughout the entire lifetime of the app
        let pending_connections = Box::leak(Box::default());

        ReverseProxyServer::new(Self {
            pending_connections,
        })
    }
}

#[tonic::async_trait]
impl ReverseProxy for ReverseProxyService {
    type BindTcpStream =
        Pin<Box<dyn Stream<Item = Result<TcpBindResponse, Status>> + Send + 'static>>;

    async fn bind_tcp(
        &self,
        request: Request<TcpBindRequest>,
    ) -> Result<Response<Self::BindTcpStream>, Status> {
        let request = request.into_inner();
        // From tokio's docs
        // "Binding with a port number of 0 will request that the OS assigns a port to this listener."
        // " The port allocated can be queried via the local_addr method."
        let port = request.port.unwrap_or(0);
        let port = utils::parse_port(port)?;

        let listener = TcpListener::bind(("0.0.0.0", port)).await.map_err(|err| {
            Status::internal(format!("failed to start a new tcp server:\n{:?}", err))
        })?;
        let port = listener.local_addr()?.port();

        // Create an async stream that accepts connections and inserts them inside a waiting queue
        let output = AcceptConnectionsStream::new(listener, port, self.pending_connections);

        Ok(Response::new(Box::pin(output) as Self::BindTcpStream))
    }

    type AcceptTcpConnectionStream =
        Pin<Box<dyn Stream<Item = Result<Packet, Status>> + Send + 'static>>;

    async fn accept_tcp_connection(
        &self,
        request: Request<Streaming<TcpAcceptRequest>>,
    ) -> Result<Response<Self::AcceptTcpConnectionStream>, Status> {
        let mut stream = request.into_inner();

        // Extract the metadata
        let metadata = stream
            .next()
            .await
            .map(|msg| {
                msg.and_then(|data| match data.request {
                    Some(tcp_accept_request::Request::Metadata(metadata)) => Ok(metadata),
                    _ => Err(Status::invalid_argument(
                        "the first message needs to contain metadata",
                    )),
                })
            })
            .ok_or_else(|| Status::cancelled("empty request"))??;
        let port = parse_port(metadata.port)?;

        // Poll a connection from the queue
        let mut conn = self
            .pending_connections
            .get_mut(&port)
            .and_then(|mut queue| queue.pop())
            .ok_or_else(|| {
                Status::invalid_argument(format!(
                    "there are no pending connections on port: {}",
                    port
                ))
            })?;

        // Create a stream that connects both ends of the connections together
        let output = async_stream::stream! {
            let mut data = vec![0u8; 4096];
            let mut client_eof = false;
            loop {
                select! {
                    rcount = conn.read(&mut data), if !client_eof => {
                        match rcount {
                            Err(_) => continue,
                            Ok(rcount) => {
                                yield Ok(Packet {
                                    data: data[..rcount].to_vec()
                                });

                                if rcount == 0 {
                                    // the client has closed its writing part,
                                    // the proxy-client is notified by receiving an empty Packet
                                    client_eof = true;
                                }
                            }
                        }
                    }

                    msg = stream.next() => {
                        let Some(msg) = msg else { break; };
                        let msg = match msg {
                            Ok(msg) => msg,
                            Err(err) => {
                                yield Err(err);
                                break;
                            }
                        };
                        let packet = match msg.request {
                            Some(tcp_accept_request::Request::Packet(packet)) => packet,
                            _ => {
                                yield Err(Status::invalid_argument("all messages, except the first one, need to contain a packet"));
                                break;
                            }
                        };

                        if let Err(err) = tokio::io::copy(&mut &packet.data[..], &mut conn).await {
                            yield Err(err.into());
                            break;
                        }
                    }
                }
            }
        };

        Ok(Response::new(
            Box::pin(output) as Self::AcceptTcpConnectionStream
        ))
    }
}

// We need to implement this stream by hand to
// be able to derive the `Drop` trait
struct AcceptConnectionsStream {
    listener: TcpListener,
    port: u16,
    queue: &'static ConnectionQueue,
    mode: AcceptConnectionsStreamMode,
}

enum AcceptConnectionsStreamMode {
    Metadata,
    Connection,
}

impl AcceptConnectionsStream {
    fn new(listener: TcpListener, port: u16, queue: &'static ConnectionQueue) -> Self {
        Self {
            listener,
            port,
            queue,
            mode: AcceptConnectionsStreamMode::Metadata,
        }
    }
}

impl Stream for AcceptConnectionsStream {
    type Item = Result<TcpBindResponse, Status>;
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        // The first message needs to contain metadata
        if matches!(self.mode, AcceptConnectionsStreamMode::Metadata) {
            let port = self.port as i32;
            std::pin::pin!(self).mode = AcceptConnectionsStreamMode::Connection;
            return Poll::Ready(Some(Ok(TcpBindResponse {
                response: Some(tcp_bind_response::Response::Metadata(
                    TcpBindResponseMetadata { port },
                )),
            })));
        }

        let (conn, _) = match std::pin::pin!(self.listener.accept()).poll(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(result) => result,
        }?;

        // save the connection in the queue and let the client know that there is a new pending connection
        self.queue.entry(self.port).or_default().push(conn);
        Poll::Ready(Some(Ok(TcpBindResponse {
            response: Some(tcp_bind_response::Response::Connection(TcpNewConnection {})),
        })))
    }
}

impl Drop for AcceptConnectionsStream {
    fn drop(&mut self) {
        // clean the queue at drop
        self.queue.remove(&self.port);
    }
}
