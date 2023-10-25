use tonic::Status;

pub fn parse_port(port: i32) -> Result<u16, Status> {
    port.try_into()
        .map_err(|_| Status::invalid_argument(format!("invalid port number: {}", port)))
}
