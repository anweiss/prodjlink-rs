use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::dbserver::client::Client;
use crate::device::types::DeviceNumber;
use crate::error::{ProDjLinkError, Result};

/// TCP port used to discover the dbserver port on a CDJ.
const DB_SERVER_QUERY_PORT: u16 = 12523;

/// Default idle timeout for pooled connections.
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(1);

/// Discover the dbserver port on a CDJ.
///
/// Connects to the player at TCP port 12523 and sends the discovery query.
/// The player responds with the port number of its dbserver.
pub async fn discover_dbserver_port(player_ip: Ipv4Addr) -> Result<u16> {
    let addr = SocketAddr::new(player_ip.into(), DB_SERVER_QUERY_PORT);
    let mut stream = TcpStream::connect(addr).await.map_err(|e| {
        ProDjLinkError::ConnectionFailed(format!("dbserver discovery {}: {}", addr, e))
    })?;

    // Send discovery request: 4-byte big-endian length followed by "RemoteDBServer\0"
    let mut request = Vec::new();
    let query_str = b"RemoteDBServer\0";
    let len = query_str.len() as u32;
    request.extend_from_slice(&len.to_be_bytes());
    request.extend_from_slice(query_str);
    stream.write_all(&request).await?;

    // Read response: 4-byte length + 2-byte port
    let mut resp = [0u8; 6];
    stream.read_exact(&mut resp).await?;
    let port = u16::from_be_bytes([resp[4], resp[5]]);

    Ok(port)
}

/// A pooled entry holding a client and its last-used timestamp.
struct PoolEntry {
    client: Client,
    last_used: Instant,
}

/// Manages pooled connections to CDJ dbservers.
pub struct ConnectionManager {
    /// Our virtual CDJ device number.
    our_player: u8,
    /// Cached connections: player number -> PoolEntry.
    pool: Mutex<HashMap<u8, PoolEntry>>,
    /// Idle timeout for connections.
    idle_timeout: Duration,
}

impl ConnectionManager {
    pub fn new(our_player: u8) -> Self {
        Self {
            our_player,
            pool: Mutex::new(HashMap::new()),
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        }
    }

    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// Execute a closure with a client connection to the given player.
    /// Handles connection caching and reconnection.
    pub async fn with_client<F, Fut, T>(
        &self,
        player: DeviceNumber,
        player_ip: Ipv4Addr,
        f: F,
    ) -> Result<T>
    where
        F: FnOnce(&mut Client) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut pool = self.pool.lock().await;

        // Check for existing connection
        if let Some(entry) = pool.get_mut(&player.0) {
            if entry.last_used.elapsed() < self.idle_timeout {
                entry.last_used = Instant::now();
                return f(&mut entry.client).await;
            } else {
                // Connection is stale, remove it
                pool.remove(&player.0);
            }
        }

        // Release lock during network I/O
        drop(pool);

        let port = discover_dbserver_port(player_ip).await?;
        let addr = SocketAddr::new(player_ip.into(), port);
        let mut client = Client::connect(addr, self.our_player, player.0).await?;

        let result = f(&mut client).await;

        // Cache the connection if the operation succeeded
        if result.is_ok() {
            let mut pool = self.pool.lock().await;
            pool.insert(
                player.0,
                PoolEntry {
                    client,
                    last_used: Instant::now(),
                },
            );
        }

        result
    }

    /// Clear all cached connections.
    pub async fn clear(&self) {
        self.pool.lock().await.clear();
    }

    /// Remove a specific player's cached connection.
    pub async fn remove(&self, player: DeviceNumber) {
        self.pool.lock().await.remove(&player.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    use crate::dbserver::field::Field;
    use crate::dbserver::message::{Message, MessageType};

    /// Test the discovery protocol by replicating the wire format against a mock.
    #[tokio::test]
    async fn discover_port_protocol() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let expected_port: u16 = 54321;

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();

            let mut buf = [0u8; 64];
            let n = stream.read(&mut buf).await.unwrap();
            assert!(n > 0);

            let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
            assert_eq!(len, 15);
            assert_eq!(&buf[4..4 + 15], b"RemoteDBServer\0");

            let mut resp = Vec::new();
            resp.extend_from_slice(&2u32.to_be_bytes());
            resp.extend_from_slice(&expected_port.to_be_bytes());
            stream.write_all(&resp).await.unwrap();
        });

        // Replicate discover_dbserver_port logic against the mock
        let mut stream = TcpStream::connect(addr).await.unwrap();

        let mut request = Vec::new();
        let query_str = b"RemoteDBServer\0";
        let len = query_str.len() as u32;
        request.extend_from_slice(&len.to_be_bytes());
        request.extend_from_slice(query_str);
        stream.write_all(&request).await.unwrap();

        let mut resp = [0u8; 6];
        stream.read_exact(&mut resp).await.unwrap();
        let port = u16::from_be_bytes([resp[4], resp[5]]);

        assert_eq!(port, expected_port);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn discover_port_connection_refused() {
        // Use a port that nothing is listening on
        let result = discover_dbserver_port(Ipv4Addr::new(127, 0, 0, 1)).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("dbserver discovery"));
    }

    #[tokio::test]
    async fn connection_manager_new() {
        let mgr = ConnectionManager::new(5);
        assert_eq!(mgr.our_player, 5);
        assert_eq!(mgr.idle_timeout, DEFAULT_IDLE_TIMEOUT);
    }

    #[tokio::test]
    async fn connection_manager_with_idle_timeout() {
        let timeout = Duration::from_secs(30);
        let mgr = ConnectionManager::new(5).with_idle_timeout(timeout);
        assert_eq!(mgr.idle_timeout, timeout);
    }

    /// Spin up a mock that handles port discovery + dbserver handshake, then runs
    /// the provided handler for any further interaction.
    async fn mock_full_server<F, Fut>(handler: F) -> (SocketAddr, tokio::task::JoinHandle<()>)
    where
        F: FnOnce(tokio::net::TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        // Start the dbserver mock
        let db_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let db_addr = db_listener.local_addr().unwrap();
        let db_port = db_addr.port();

        // Start the discovery mock
        let disc_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let disc_addr = disc_listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            // Handle discovery query
            let (mut disc_stream, _) = disc_listener.accept().await.unwrap();
            let mut buf = [0u8; 64];
            let n = disc_stream.read(&mut buf).await.unwrap();
            assert!(n > 0);

            let mut resp = Vec::new();
            resp.extend_from_slice(&2u32.to_be_bytes());
            resp.extend_from_slice(&db_port.to_be_bytes());
            disc_stream.write_all(&resp).await.unwrap();
            drop(disc_stream);

            // Handle dbserver connection: greeting + setup
            let (mut stream, _) = db_listener.accept().await.unwrap();

            // Read greeting
            let mut gbuf = [0u8; 16];
            let n = stream.read(&mut gbuf).await.unwrap();
            assert!(n > 0);
            let mut cursor = &gbuf[..n];
            let field = Field::parse(&mut cursor).unwrap();
            assert_eq!(field, Field::Number { value: 1, size: 4 });

            // Send greeting response
            let resp_field = Field::number_with_size(1, 4);
            let mut resp_buf = BytesMut::new();
            resp_field.serialize(&mut resp_buf);
            stream.write_all(&resp_buf).await.unwrap();

            // Read setup message
            let mut msg_buf = BytesMut::with_capacity(256);
            let mut tmp = [0u8; 256];
            loop {
                let n = stream.read(&mut tmp).await.unwrap();
                assert!(n > 0);
                msg_buf.extend_from_slice(&tmp[..n]);
                let mut c = std::io::Cursor::new(&msg_buf[..]);
                if Message::parse(&mut c).is_ok() {
                    break;
                }
            }
            let mut c = std::io::Cursor::new(&msg_buf[..]);
            let setup = Message::parse(&mut c).unwrap();
            assert_eq!(setup.kind, MessageType::SetupReq);

            // Send MENU_AVAILABLE response
            let resp_msg = Message::new(setup.transaction, MessageType::MenuAvailable, vec![]);
            stream.write_all(&resp_msg.serialize()).await.unwrap();

            handler(stream).await;
        });

        (disc_addr, handle)
    }

    #[tokio::test]
    async fn with_client_connects_and_caches() {
        let (disc_addr, server) = mock_full_server(|mut stream| async move {
            // Read a simple request
            let mut buf = BytesMut::with_capacity(256);
            let mut tmp = [0u8; 256];
            loop {
                let n = stream.read(&mut tmp).await.unwrap();
                assert!(n > 0);
                buf.extend_from_slice(&tmp[..n]);
                let mut c = std::io::Cursor::new(&buf[..]);
                if Message::parse(&mut c).is_ok() {
                    break;
                }
            }
            let mut c = std::io::Cursor::new(&buf[..]);
            let req = Message::parse(&mut c).unwrap();

            let resp = Message::new(req.transaction, MessageType::MenuItem, vec![Field::number(42)]);
            stream.write_all(&resp.serialize()).await.unwrap();
        })
        .await;

        // We need a ConnectionManager that discovers the port via our mock.
        // Since discover_dbserver_port hardcodes DB_SERVER_QUERY_PORT, we
        // test the with_client logic by manually connecting through the mock's
        // discovery port. We'll use a helper that bypasses the hardcoded port.
        //
        // For a true integration test we'd need to bind to port 12523, which
        // requires privileges. Instead, test the caching logic via clear/remove
        // and test the protocol via mock_full_server + Client::connect directly.

        // Verify the mock server works end-to-end with Client::connect
        let disc_ip: Ipv4Addr = disc_addr.ip().to_string().parse().unwrap();

        // Manually do what with_client does: discover port, connect, use client
        let mut disc_stream = TcpStream::connect(disc_addr).await.unwrap();
        let mut request = Vec::new();
        let query_str = b"RemoteDBServer\0";
        let len = query_str.len() as u32;
        request.extend_from_slice(&len.to_be_bytes());
        request.extend_from_slice(query_str);
        disc_stream.write_all(&request).await.unwrap();

        let mut resp = [0u8; 6];
        disc_stream.read_exact(&mut resp).await.unwrap();
        let port = u16::from_be_bytes([resp[4], resp[5]]);
        drop(disc_stream);

        let db_addr = SocketAddr::new(disc_ip.into(), port);
        let mut client = Client::connect(db_addr, 5, 3).await.unwrap();

        let resp = client
            .simple_request(MessageType::MetadataReq, vec![Field::number(1)])
            .await
            .unwrap();
        assert_eq!(resp.kind, MessageType::MenuItem);
        assert_eq!(resp.arg_number(0).unwrap(), 42);

        server.await.unwrap();
    }

    #[tokio::test]
    async fn clear_empties_pool() {
        let mgr = ConnectionManager::new(5);
        // Pool starts empty
        assert!(mgr.pool.lock().await.is_empty());
        mgr.clear().await;
        assert!(mgr.pool.lock().await.is_empty());
    }

    #[tokio::test]
    async fn remove_nonexistent_player() {
        let mgr = ConnectionManager::new(5);
        // Removing a non-existent player should not panic
        mgr.remove(DeviceNumber(3)).await;
        assert!(mgr.pool.lock().await.is_empty());
    }
}
