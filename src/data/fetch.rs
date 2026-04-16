use std::net::Ipv4Addr;

use crate::data::artwork::{
    AlbumArt, ArtworkReference, build_art_request_args, extract_art_from_response,
};
use crate::data::beatgrid::BeatGrid;
use crate::data::cue::CueList;
use crate::data::metadata::{DataReference, TrackMetadata, build_metadata_request_args};
use crate::data::waveform::{WaveformDetail, WaveformPreview, WaveformStyle};
use crate::dbserver::connection::ConnectionManager;
use crate::dbserver::field::Field;
use crate::dbserver::message::MessageType;
use crate::device::types::{DeviceNumber, TrackSourceSlot};
use crate::error::{ProDjLinkError, Result};

/// Menu identifier value used in dbserver data requests.
const MENU_ID_DATA: u8 = 8;

// -----------------------------------------------------------------------
// Internal async fn helpers — these accept &mut Client directly so the
// compiler can tie the future's lifetime to the borrow.
// -----------------------------------------------------------------------

async fn do_metadata(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
    data_ref: DataReference,
) -> Result<TrackMetadata> {
    let items = client.menu_request(MessageType::MetadataReq, args).await?;
    Ok(TrackMetadata::from_menu_items(data_ref, &items))
}

async fn do_artwork(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
    art_ref: ArtworkReference,
) -> Result<AlbumArt> {
    let resp = client
        .simple_request(MessageType::AlbumArtReq, args)
        .await?;
    extract_art_from_response(art_ref, &resp)
}

async fn do_beatgrid(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
) -> Result<BeatGrid> {
    let resp = client
        .simple_request(MessageType::BeatGridReq, args)
        .await?;
    let data = resp
        .args
        .get(3)
        .ok_or_else(|| ProDjLinkError::Parse("missing beat grid data in response".into()))?
        .as_binary()?;
    BeatGrid::from_bytes(data)
}

async fn do_cue_list(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
) -> Result<CueList> {
    let items = client
        .menu_request(MessageType::CueListExtReq, args)
        .await?;
    Ok(CueList::from_menu_items(&items))
}

async fn do_waveform_preview(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
) -> Result<WaveformPreview> {
    let resp = client
        .simple_request(MessageType::WaveformPreviewReq, args)
        .await?;
    let data = resp
        .args
        .get(3)
        .ok_or_else(|| ProDjLinkError::Parse("missing waveform preview data in response".into()))?
        .as_binary()?
        .clone();
    WaveformPreview::from_bytes(data, WaveformStyle::Blue)
}

async fn do_waveform_detail(
    client: &mut crate::dbserver::client::Client,
    args: Vec<Field>,
) -> Result<WaveformDetail> {
    let resp = client
        .simple_request(MessageType::WaveformDetailReq, args)
        .await?;
    let data = resp
        .args
        .get(3)
        .ok_or_else(|| ProDjLinkError::Parse("missing waveform detail data in response".into()))?
        .as_binary()?
        .clone();
    WaveformDetail::from_bytes(data, WaveformStyle::Blue)
}

// -----------------------------------------------------------------------
// Public one-shot fetch functions
// -----------------------------------------------------------------------

/// Fetch track metadata from a player.
pub async fn fetch_metadata(
    conn: &ConnectionManager,
    player: DeviceNumber,
    player_ip: Ipv4Addr,
    slot: TrackSourceSlot,
    track_id: u32,
) -> Result<TrackMetadata> {
    let data_ref = DataReference::new(player, slot, track_id);
    let args = build_metadata_request_args(&data_ref, MENU_ID_DATA);
    conn.with_client(player, player_ip, |client| {
        Box::pin(do_metadata(client, args, data_ref))
    })
    .await
}

/// Fetch album art from a player.
pub async fn fetch_artwork(
    conn: &ConnectionManager,
    player: DeviceNumber,
    player_ip: Ipv4Addr,
    slot: TrackSourceSlot,
    artwork_id: u32,
) -> Result<AlbumArt> {
    let art_ref = ArtworkReference {
        player,
        slot,
        artwork_id,
    };
    let args = build_art_request_args(&art_ref);
    conn.with_client(player, player_ip, |client| {
        Box::pin(do_artwork(client, args, art_ref))
    })
    .await
}

/// Fetch beat grid from a player.
pub async fn fetch_beatgrid(
    conn: &ConnectionManager,
    player: DeviceNumber,
    player_ip: Ipv4Addr,
    slot: TrackSourceSlot,
    track_id: u32,
) -> Result<BeatGrid> {
    let args = vec![
        Field::number(MENU_ID_DATA as u32),
        Field::number(u8::from(slot) as u32),
        Field::number(track_id),
    ];
    conn.with_client(player, player_ip, |client| {
        Box::pin(do_beatgrid(client, args))
    })
    .await
}

/// Fetch cue list from a player.
pub async fn fetch_cue_list(
    conn: &ConnectionManager,
    player: DeviceNumber,
    player_ip: Ipv4Addr,
    slot: TrackSourceSlot,
    track_id: u32,
) -> Result<CueList> {
    let args = vec![
        Field::number(MENU_ID_DATA as u32),
        Field::number(u8::from(slot) as u32),
        Field::number(track_id),
    ];
    conn.with_client(player, player_ip, |client| {
        Box::pin(do_cue_list(client, args))
    })
    .await
}

/// Fetch waveform preview from a player.
pub async fn fetch_waveform_preview(
    conn: &ConnectionManager,
    player: DeviceNumber,
    player_ip: Ipv4Addr,
    slot: TrackSourceSlot,
    track_id: u32,
) -> Result<WaveformPreview> {
    let args = vec![
        Field::number(MENU_ID_DATA as u32),
        Field::number(u8::from(slot) as u32),
        Field::number(track_id),
    ];
    conn.with_client(player, player_ip, |client| {
        Box::pin(do_waveform_preview(client, args))
    })
    .await
}

/// Fetch detailed waveform from a player.
pub async fn fetch_waveform_detail(
    conn: &ConnectionManager,
    player: DeviceNumber,
    player_ip: Ipv4Addr,
    slot: TrackSourceSlot,
    track_id: u32,
) -> Result<WaveformDetail> {
    let args = vec![
        Field::number(MENU_ID_DATA as u32),
        Field::number(u8::from(slot) as u32),
        Field::number(track_id),
    ];
    conn.with_client(player, player_ip, |client| {
        Box::pin(do_waveform_detail(client, args))
    })
    .await
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use crate::dbserver::client::Client;
    use crate::dbserver::field::Field;
    use crate::dbserver::message::{Message, MessageType};

    // -- Mock server infrastructure --

    /// Spin up a mock that handles port discovery + dbserver handshake, then
    /// runs the provided handler for any further interaction. Returns the
    /// discovery address and a join handle.
    async fn mock_full_server<F, Fut>(
        handler: F,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>)
    where
        F: FnOnce(tokio::net::TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let db_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let db_addr = db_listener.local_addr().unwrap();
        let db_port = db_addr.port();

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

    /// Helper: read one message from a raw stream.
    async fn read_one_message(stream: &mut tokio::net::TcpStream) -> Message {
        let mut buf = BytesMut::with_capacity(4096);
        let mut tmp = [0u8; 4096];
        loop {
            let n = stream.read(&mut tmp).await.unwrap();
            assert!(n > 0);
            buf.extend_from_slice(&tmp[..n]);
            let mut c = std::io::Cursor::new(&buf[..]);
            if let Ok(msg) = Message::parse(&mut c) {
                return msg;
            }
        }
    }

    // -- Metadata fetch test --

    #[tokio::test]
    async fn fetch_metadata_parses_menu_items() {
        let (disc_addr, server) = mock_full_server(|mut stream| async move {
            let req = read_one_message(&mut stream).await;
            assert_eq!(req.kind, MessageType::MetadataReq);

            let txn = req.transaction;

            // Build menu items: header + title item + footer
            let header = Message::new(txn, MessageType::MenuHeader, vec![]);
            let title_item = Message::new(
                txn,
                MessageType::MenuItem,
                vec![
                    Field::number(0),            // arg 0
                    Field::number(0),            // arg 1
                    Field::number(0),            // arg 2
                    Field::string("Test Track"), // arg 3: text
                    Field::number(0),            // arg 4
                    Field::string(""),           // arg 5
                    Field::number(0x0004),       // arg 6: TrackTitle
                ],
            );
            let footer = Message::new(txn, MessageType::MenuFooter, vec![]);

            let mut out = BytesMut::new();
            out.extend_from_slice(&header.serialize());
            out.extend_from_slice(&title_item.serialize());
            out.extend_from_slice(&footer.serialize());
            stream.write_all(&out).await.unwrap();
        })
        .await;

        // Bypass the ConnectionManager (which requires port 12523) and
        // manually connect via the mock discovery address.
        let disc_ip: Ipv4Addr = disc_addr.ip().to_string().parse().unwrap();

        // Discover the port
        let mut disc_stream = tokio::net::TcpStream::connect(disc_addr).await.unwrap();
        let mut request = Vec::new();
        let query_str = b"RemoteDBServer\0";
        request.extend_from_slice(&(query_str.len() as u32).to_be_bytes());
        request.extend_from_slice(query_str);
        disc_stream.write_all(&request).await.unwrap();
        let mut resp = [0u8; 6];
        disc_stream.read_exact(&mut resp).await.unwrap();
        let port = u16::from_be_bytes([resp[4], resp[5]]);
        drop(disc_stream);

        let db_addr = std::net::SocketAddr::new(disc_ip.into(), port);
        let mut client = Client::connect(db_addr, 5, 3).await.unwrap();

        let data_ref = DataReference::new(DeviceNumber(3), TrackSourceSlot::UsbSlot, 42);
        let args = build_metadata_request_args(&data_ref, MENU_ID_DATA);
        let items = client
            .menu_request(MessageType::MetadataReq, args)
            .await
            .unwrap();
        let meta = TrackMetadata::from_menu_items(data_ref, &items);

        assert_eq!(meta.title, "Test Track");
        assert_eq!(meta.data_ref, data_ref);

        server.await.unwrap();
    }

    // -- Artwork fetch test --

    #[tokio::test]
    async fn fetch_artwork_extracts_image() {
        let (disc_addr, server) = mock_full_server(|mut stream| async move {
            let req = read_one_message(&mut stream).await;
            assert_eq!(req.kind, MessageType::AlbumArtReq);

            let jpeg_data = bytes::Bytes::from_static(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]);
            let resp = Message::new(
                req.transaction,
                MessageType::AlbumArtResponse,
                vec![
                    Field::number(0),
                    Field::number(0),
                    Field::number(0),
                    Field::binary(jpeg_data),
                ],
            );
            stream.write_all(&resp.serialize()).await.unwrap();
        })
        .await;

        let disc_ip: Ipv4Addr = disc_addr.ip().to_string().parse().unwrap();
        let mut disc_stream = tokio::net::TcpStream::connect(disc_addr).await.unwrap();
        let mut request = Vec::new();
        let query_str = b"RemoteDBServer\0";
        request.extend_from_slice(&(query_str.len() as u32).to_be_bytes());
        request.extend_from_slice(query_str);
        disc_stream.write_all(&request).await.unwrap();
        let mut resp = [0u8; 6];
        disc_stream.read_exact(&mut resp).await.unwrap();
        let port = u16::from_be_bytes([resp[4], resp[5]]);
        drop(disc_stream);

        let db_addr = std::net::SocketAddr::new(disc_ip.into(), port);
        let mut client = Client::connect(db_addr, 5, 3).await.unwrap();

        let art_ref = ArtworkReference {
            player: DeviceNumber(3),
            slot: TrackSourceSlot::UsbSlot,
            artwork_id: 55,
        };
        let args = build_art_request_args(&art_ref);
        let resp = client
            .simple_request(MessageType::AlbumArtReq, args)
            .await
            .unwrap();
        let art = extract_art_from_response(art_ref, &resp).unwrap();

        assert!(art.is_jpeg());
        assert_eq!(art.art_ref.artwork_id, 55);

        server.await.unwrap();
    }

    // -- Beat grid fetch test --

    #[tokio::test]
    async fn fetch_beatgrid_parses_response() {
        let (disc_addr, server) = mock_full_server(|mut stream| async move {
            let req = read_one_message(&mut stream).await;
            assert_eq!(req.kind, MessageType::BeatGridReq);

            // Build beat grid binary data: 20-byte header + 1 entry (16 bytes)
            let mut grid_data = vec![0u8; 20]; // header
            let mut entry = vec![0u8; 16];
            entry[0..2].copy_from_slice(&1u16.to_le_bytes()); // beat_within_bar
            entry[2..4].copy_from_slice(&12800u16.to_le_bytes()); // tempo_raw (128.00)
            entry[4..8].copy_from_slice(&0u32.to_le_bytes()); // time_ms
            grid_data.extend_from_slice(&entry);

            let resp = Message::new(
                req.transaction,
                MessageType::BeatGridResponse,
                vec![
                    Field::number(0),
                    Field::number(0),
                    Field::number(0),
                    Field::binary(bytes::Bytes::from(grid_data)),
                ],
            );
            stream.write_all(&resp.serialize()).await.unwrap();
        })
        .await;

        let disc_ip: Ipv4Addr = disc_addr.ip().to_string().parse().unwrap();
        let mut disc_stream = tokio::net::TcpStream::connect(disc_addr).await.unwrap();
        let mut request = Vec::new();
        let query_str = b"RemoteDBServer\0";
        request.extend_from_slice(&(query_str.len() as u32).to_be_bytes());
        request.extend_from_slice(query_str);
        disc_stream.write_all(&request).await.unwrap();
        let mut resp = [0u8; 6];
        disc_stream.read_exact(&mut resp).await.unwrap();
        let port = u16::from_be_bytes([resp[4], resp[5]]);
        drop(disc_stream);

        let db_addr = std::net::SocketAddr::new(disc_ip.into(), port);
        let mut client = Client::connect(db_addr, 5, 3).await.unwrap();

        let args = vec![
            Field::number(MENU_ID_DATA as u32),
            Field::number(3), // UsbSlot
            Field::number(42),
        ];
        let resp = client
            .simple_request(MessageType::BeatGridReq, args)
            .await
            .unwrap();
        let data = resp.args.get(3).unwrap().as_binary().unwrap();
        let grid = BeatGrid::from_bytes(data).unwrap();

        assert_eq!(grid.len(), 1);
        assert_eq!(grid.entries[0].beat_within_bar, 1);
        assert!((grid.entries[0].tempo - 128.0).abs() < f64::EPSILON);

        server.await.unwrap();
    }

    // -- Error handling tests --

    #[tokio::test]
    async fn connection_refused_error() {
        let addr: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
        let err = Client::connect(addr, 5, 3).await.unwrap_err();
        assert!(err.to_string().contains("connection failed"));
    }

    #[tokio::test]
    async fn malformed_response_missing_art_field() {
        let (disc_addr, server) = mock_full_server(|mut stream| async move {
            let req = read_one_message(&mut stream).await;
            // Respond with too few args — no binary field at index 3
            let resp = Message::new(
                req.transaction,
                MessageType::AlbumArtResponse,
                vec![Field::number(0), Field::number(0)],
            );
            stream.write_all(&resp.serialize()).await.unwrap();
        })
        .await;

        let disc_ip: Ipv4Addr = disc_addr.ip().to_string().parse().unwrap();
        let mut disc_stream = tokio::net::TcpStream::connect(disc_addr).await.unwrap();
        let mut request = Vec::new();
        let query_str = b"RemoteDBServer\0";
        request.extend_from_slice(&(query_str.len() as u32).to_be_bytes());
        request.extend_from_slice(query_str);
        disc_stream.write_all(&request).await.unwrap();
        let mut resp = [0u8; 6];
        disc_stream.read_exact(&mut resp).await.unwrap();
        let port = u16::from_be_bytes([resp[4], resp[5]]);
        drop(disc_stream);

        let db_addr = std::net::SocketAddr::new(disc_ip.into(), port);
        let mut client = Client::connect(db_addr, 5, 3).await.unwrap();

        let art_ref = ArtworkReference {
            player: DeviceNumber(3),
            slot: TrackSourceSlot::UsbSlot,
            artwork_id: 55,
        };
        let args = build_art_request_args(&art_ref);
        let resp = client
            .simple_request(MessageType::AlbumArtReq, args)
            .await
            .unwrap();
        let err = extract_art_from_response(art_ref, &resp).unwrap_err();
        assert!(err.to_string().contains("missing art data"));

        server.await.unwrap();
    }

    #[tokio::test]
    async fn data_reference_translates_to_request_args() {
        let data_ref = DataReference::new(DeviceNumber(2), TrackSourceSlot::SdSlot, 100);
        let args = build_metadata_request_args(&data_ref, MENU_ID_DATA);

        assert_eq!(args.len(), 3);
        assert_eq!(args[0].as_number().unwrap(), 8); // MENU_ID_DATA
        assert_eq!(args[1].as_number().unwrap(), 2); // SdSlot
        assert_eq!(args[2].as_number().unwrap(), 100); // rekordbox_id

        // Also verify the direct field building for non-metadata requests
        let direct_args = [
            Field::number(MENU_ID_DATA as u32),
            Field::number(u8::from(data_ref.slot) as u32),
            Field::number(data_ref.rekordbox_id),
        ];
        assert_eq!(direct_args[0].as_number().unwrap(), 8);
        assert_eq!(direct_args[1].as_number().unwrap(), 2);
        assert_eq!(direct_args[2].as_number().unwrap(), 100);
    }
}
