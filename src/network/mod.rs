pub mod beat;
pub mod finder;
pub mod interface;
pub mod status;
pub mod tempo;
pub mod time;
pub mod virtual_cdj;

/// Create a UDP socket bound to `0.0.0.0:{port}` with `SO_REUSEADDR` +
/// `SO_REUSEPORT` so multiple DJ Link instances can coexist on one machine.
fn create_reuseport_socket(port: u16) -> std::io::Result<tokio::net::UdpSocket> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_reuse_address(true)?;
    #[cfg(not(windows))]
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
    socket.bind(&addr.into())?;
    let std_socket: std::net::UdpSocket = socket.into();
    tokio::net::UdpSocket::from_std(std_socket)
}
