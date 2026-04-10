use std::net::SocketAddr;

use bytes::{Buf, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpStream;

use crate::dbserver::field::Field;
use crate::dbserver::message::{Message, MessageType};
use crate::error::{ProDjLinkError, Result};

/// A client connection to a Pioneer CDJ's dbserver.
#[derive(Debug)]
pub struct Client {
    reader: BufReader<tokio::io::ReadHalf<TcpStream>>,
    writer: BufWriter<tokio::io::WriteHalf<TcpStream>>,
    /// Accumulated read buffer (may contain partial next message).
    read_buf: BytesMut,
    /// Incrementing transaction ID.
    next_transaction: u32,
    /// The target player number this client is connected to.
    target_player: u8,
}

impl Client {
    /// Connect to a dbserver at the given address and perform the handshake.
    ///
    /// `our_player_number` is the device number we're presenting as (our VirtualCdj number).
    /// `target_player` is the device number of the CDJ we're connecting to.
    pub async fn connect(
        addr: SocketAddr,
        our_player_number: u8,
        target_player: u8,
    ) -> Result<Self> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| ProDjLinkError::ConnectionFailed(format!("{addr}: {e}")))?;

        let (read_half, write_half) = tokio::io::split(stream);
        let mut reader = BufReader::new(read_half);
        let mut writer = BufWriter::new(write_half);

        // Greeting: send NumberField(1, size=4) as a raw field (not wrapped in a Message).
        let greeting = Field::number_with_size(1, 4);
        let mut greeting_buf = BytesMut::new();
        greeting.serialize(&mut greeting_buf);
        writer.write_all(&greeting_buf).await?;
        writer.flush().await?;

        // Read greeting response: expect NumberField(1).
        let mut resp_buf = BytesMut::zeroed(16);
        let n = reader.read(&mut resp_buf[..]).await?;
        if n == 0 {
            return Err(ProDjLinkError::ConnectionFailed(
                "connection closed during greeting".into(),
            ));
        }
        let mut cursor = &resp_buf[..n];
        let resp_field = Field::parse(&mut cursor).map_err(|e| {
            ProDjLinkError::ConnectionFailed(format!("greeting parse failed: {e}"))
        })?;
        match resp_field {
            Field::Number { value: 1, .. } => {}
            _ => {
                return Err(ProDjLinkError::ConnectionFailed(format!(
                    "unexpected greeting response: {resp_field:?}"
                )));
            }
        }

        let mut client = Self {
            reader,
            writer,
            read_buf: BytesMut::with_capacity(4096),
            next_transaction: 1,
            target_player,
        };

        // Setup handshake: send SETUP_REQ, expect MENU_AVAILABLE.
        let setup_msg = Message::new(
            client.next_transaction(),
            MessageType::SetupReq,
            vec![Field::number(our_player_number as u32)],
        );
        let setup_resp = client.send_message(setup_msg).await?;
        if setup_resp.kind != MessageType::MenuAvailable {
            return Err(ProDjLinkError::DbServer(format!(
                "expected MenuAvailable, got {:?}",
                setup_resp.kind
            )));
        }

        Ok(client)
    }

    /// Return the target player number.
    pub fn target_player(&self) -> u8 {
        self.target_player
    }

    /// Get and increment the transaction ID.
    fn next_transaction(&mut self) -> u32 {
        let id = self.next_transaction;
        self.next_transaction = self.next_transaction.wrapping_add(1);
        id
    }

    /// Send a message and read the response.
    pub async fn send_message(&mut self, msg: Message) -> Result<Message> {
        self.send_message_raw(&msg).await?;
        self.read_message().await
    }

    /// Send a message without reading the response.
    async fn send_message_raw(&mut self, msg: &Message) -> Result<()> {
        let data = msg.serialize();
        self.writer.write_all(&data).await?;
        self.writer.flush().await?;
        Ok(())
    }

    /// Read a single message from the stream.
    ///
    /// Uses the persistent `read_buf` so that leftover bytes from a previous
    /// read are not lost between calls.
    async fn read_message(&mut self) -> Result<Message> {
        loop {
            // Minimum message size: magic(4) + transaction(4) + type(2) + argcount(1) = 11
            if self.read_buf.len() >= 11 {
                let mut cursor = std::io::Cursor::new(&self.read_buf[..]);
                match Message::parse(&mut cursor) {
                    Ok(msg) => {
                        let consumed = cursor.position() as usize;
                        self.read_buf.advance(consumed);
                        return Ok(msg);
                    }
                    Err(_) => {
                        // Not enough data yet, fall through to read more.
                    }
                }
            }

            let mut tmp = [0u8; 4096];
            let n = self.reader.read(&mut tmp).await?;
            if n == 0 {
                return Err(ProDjLinkError::ConnectionFailed(
                    "connection closed".into(),
                ));
            }
            self.read_buf.extend_from_slice(&tmp[..n]);
        }
    }

    /// Send a request and verify the response transaction ID matches.
    pub async fn simple_request(
        &mut self,
        kind: MessageType,
        args: Vec<Field>,
    ) -> Result<Message> {
        let txn = self.next_transaction();
        let msg = Message::new(txn, kind, args);
        let resp = self.send_message(msg).await?;

        if resp.transaction != txn {
            return Err(ProDjLinkError::DbServer(format!(
                "transaction mismatch: expected {txn}, got {}",
                resp.transaction
            )));
        }

        Ok(resp)
    }

    /// Send a menu request and collect all MenuItems between MenuHeader and MenuFooter.
    pub async fn menu_request(
        &mut self,
        kind: MessageType,
        args: Vec<Field>,
    ) -> Result<Vec<Message>> {
        let txn = self.next_transaction();
        let msg = Message::new(txn, kind, args);
        self.send_message_raw(&msg).await?;

        let mut items = Vec::new();
        loop {
            let resp = self.read_message().await?;
            match resp.kind {
                MessageType::MenuHeader => continue,
                MessageType::MenuItem => items.push(resp),
                MessageType::MenuFooter => break,
                _ => {
                    // Single non-menu response; return it directly.
                    items.push(resp);
                    break;
                }
            }
        }

        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    /// Spin up a mock dbserver that performs the greeting + setup handshake,
    /// then runs `handler` for any further interaction.
    async fn mock_server<F, Fut>(handler: F) -> (SocketAddr, tokio::task::JoinHandle<()>)
    where
        F: FnOnce(tokio::net::TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();

            // --- greeting ---
            // Read the greeting field
            let mut buf = [0u8; 16];
            let n = stream.read(&mut buf).await.unwrap();
            assert!(n > 0);
            let mut cursor = &buf[..n];
            let field = Field::parse(&mut cursor).unwrap();
            assert_eq!(field, Field::Number { value: 1, size: 4 });

            // Send greeting response
            let resp = Field::number_with_size(1, 4);
            let mut resp_buf = BytesMut::new();
            resp.serialize(&mut resp_buf);
            stream.write_all(&resp_buf).await.unwrap();

            // --- setup ---
            // Read the SETUP_REQ message
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

            // Send MENU_AVAILABLE response with matching transaction
            let resp_msg = Message::new(setup.transaction, MessageType::MenuAvailable, vec![]);
            let data = resp_msg.serialize();
            stream.write_all(&data).await.unwrap();

            handler(stream).await;
        });

        (addr, handle)
    }

    #[tokio::test]
    async fn connect_handshake() {
        let (addr, server) = mock_server(|_stream| async {}).await;

        let client = Client::connect(addr, 5, 3).await.unwrap();
        assert_eq!(client.target_player(), 3);
        // Transaction 1 was used for setup, so next should be 2.
        assert_eq!(client.next_transaction, 2);

        server.await.unwrap();
    }

    #[tokio::test]
    async fn simple_request_round_trip() {
        let (addr, server) = mock_server(|mut stream| async move {
            // Read the request
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
            assert_eq!(req.kind, MessageType::MetadataReq);

            // Echo back with same transaction
            let resp = Message::new(req.transaction, MessageType::MenuItem, req.args);
            stream.write_all(&resp.serialize()).await.unwrap();
        })
        .await;

        let mut client = Client::connect(addr, 5, 3).await.unwrap();
        let resp = client
            .simple_request(
                MessageType::MetadataReq,
                vec![Field::number(42)],
            )
            .await
            .unwrap();

        assert_eq!(resp.kind, MessageType::MenuItem);
        assert_eq!(resp.arg_number(0).unwrap(), 42);

        server.await.unwrap();
    }

    #[tokio::test]
    async fn simple_request_transaction_mismatch() {
        let (addr, server) = mock_server(|mut stream| async move {
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

            // Respond with wrong transaction ID
            let resp = Message::new(req.transaction + 999, MessageType::MenuItem, vec![]);
            stream.write_all(&resp.serialize()).await.unwrap();
        })
        .await;

        let mut client = Client::connect(addr, 5, 3).await.unwrap();
        let err = client
            .simple_request(MessageType::MetadataReq, vec![])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("transaction mismatch"));

        server.await.unwrap();
    }

    #[tokio::test]
    async fn menu_request_collects_items() {
        let (addr, server) = mock_server(|mut stream| async move {
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
            let txn = req.transaction;

            // Send MenuHeader + 3 MenuItems + MenuFooter
            let header = Message::new(txn, MessageType::MenuHeader, vec![]);
            let item1 = Message::new(
                txn,
                MessageType::MenuItem,
                vec![Field::string("Track A")],
            );
            let item2 = Message::new(
                txn,
                MessageType::MenuItem,
                vec![Field::string("Track B")],
            );
            let item3 = Message::new(
                txn,
                MessageType::MenuItem,
                vec![Field::string("Track C")],
            );
            let footer = Message::new(txn, MessageType::MenuFooter, vec![]);

            let mut out = BytesMut::new();
            out.extend_from_slice(&header.serialize());
            out.extend_from_slice(&item1.serialize());
            out.extend_from_slice(&item2.serialize());
            out.extend_from_slice(&item3.serialize());
            out.extend_from_slice(&footer.serialize());
            stream.write_all(&out).await.unwrap();
        })
        .await;

        let mut client = Client::connect(addr, 5, 3).await.unwrap();
        let items = client
            .menu_request(MessageType::RenderMenuReq, vec![Field::number(1)])
            .await
            .unwrap();

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].arg_string(0).unwrap(), "Track A");
        assert_eq!(items[1].arg_string(0).unwrap(), "Track B");
        assert_eq!(items[2].arg_string(0).unwrap(), "Track C");

        server.await.unwrap();
    }

    #[tokio::test]
    async fn menu_request_single_response() {
        let (addr, server) = mock_server(|mut stream| async move {
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

            // Respond with a non-menu message (e.g. error / single result)
            let resp = Message::new(
                req.transaction,
                MessageType::MenuAvailable,
                vec![Field::number(0)],
            );
            stream.write_all(&resp.serialize()).await.unwrap();
        })
        .await;

        let mut client = Client::connect(addr, 5, 3).await.unwrap();
        let items = client
            .menu_request(MessageType::RenderMenuReq, vec![])
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, MessageType::MenuAvailable);

        server.await.unwrap();
    }

    #[tokio::test]
    async fn connect_refused() {
        // Connect to a port that nothing is listening on.
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let err = Client::connect(addr, 5, 3).await.unwrap_err();
        assert!(err.to_string().contains("connection failed"));
    }
}
