use bytes::{Buf, BufMut, BytesMut};

use super::Field;
use crate::error::{ProDjLinkError, Result};

/// Magic bytes at the start of every dbserver message.
pub const MESSAGE_START: u32 = 0x872349ae;
/// Maximum number of arguments per message.
pub const MAX_ARGS: usize = 12;

/// Known dbserver message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    SetupReq,
    MenuAvailable,
    MenuHeader,
    MenuItem,
    MenuFooter,
    MetadataReq,
    AlbumArtReq,
    WaveformPreviewReq,
    WaveformDetailReq,
    CueListReq,
    CueListExtReq,
    BeatGridReq,
    AnlzTagReq,
    RenderMenuReq,
    Unknown(u16),
}

impl From<u16> for MessageType {
    fn from(value: u16) -> Self {
        match value {
            0x0000 => MessageType::SetupReq,
            0x4000 => MessageType::MenuAvailable,
            0x4001 => MessageType::MenuHeader,
            0x4101 => MessageType::MenuItem,
            0x4201 => MessageType::MenuFooter,
            0x2002 => MessageType::MetadataReq,
            0x2003 => MessageType::AlbumArtReq,
            0x2004 => MessageType::WaveformPreviewReq,
            0x2904 => MessageType::WaveformDetailReq,
            0x2104 => MessageType::CueListReq,
            0x2b04 => MessageType::CueListExtReq,
            0x2204 => MessageType::BeatGridReq,
            0x2008 => MessageType::AnlzTagReq,
            0x3000 => MessageType::RenderMenuReq,
            other => MessageType::Unknown(other),
        }
    }
}

impl From<MessageType> for u16 {
    fn from(mt: MessageType) -> u16 {
        match mt {
            MessageType::SetupReq => 0x0000,
            MessageType::MenuAvailable => 0x4000,
            MessageType::MenuHeader => 0x4001,
            MessageType::MenuItem => 0x4101,
            MessageType::MenuFooter => 0x4201,
            MessageType::MetadataReq => 0x2002,
            MessageType::AlbumArtReq => 0x2003,
            MessageType::WaveformPreviewReq => 0x2004,
            MessageType::WaveformDetailReq => 0x2904,
            MessageType::CueListReq => 0x2104,
            MessageType::CueListExtReq => 0x2b04,
            MessageType::BeatGridReq => 0x2204,
            MessageType::AnlzTagReq => 0x2008,
            MessageType::RenderMenuReq => 0x3000,
            MessageType::Unknown(v) => v,
        }
    }
}

/// Menu identifiers used in menu requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MenuIdentifier {
    MainMenu = 1,
    SubMenu = 2,
    TrackInfo = 3,
    SortMenu = 5,
    Data = 8,
}

/// Menu item types that identify what kind of data a menu item represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuItemType {
    TrackTitle,
    Artist,
    AlbumTitle,
    Genre,
    Comment,
    Tempo,
    Rating,
    ColorLabel,
    Key,
    DateAdded,
    Unknown(u16),
}

impl From<u16> for MenuItemType {
    fn from(value: u16) -> Self {
        match value {
            0x0001 => MenuItemType::TrackTitle,
            0x0002 => MenuItemType::Artist,
            0x0003 => MenuItemType::AlbumTitle,
            0x0006 => MenuItemType::Genre,
            0x0009 => MenuItemType::Comment,
            0x000a => MenuItemType::Tempo,
            0x000b => MenuItemType::Rating,
            0x000d => MenuItemType::ColorLabel,
            0x000e => MenuItemType::Key,
            0x0010 => MenuItemType::DateAdded,
            other => MenuItemType::Unknown(other),
        }
    }
}

/// A dbserver protocol message.
#[derive(Debug, Clone)]
pub struct Message {
    /// Transaction ID for request/response matching.
    pub transaction: u32,
    /// The message type.
    pub kind: MessageType,
    /// The arguments (0–12 typed fields).
    pub args: Vec<Field>,
}

impl Message {
    /// Create a new message.
    pub fn new(transaction: u32, kind: MessageType, args: Vec<Field>) -> Self {
        Self {
            transaction,
            kind,
            args,
        }
    }

    /// Parse a message from a byte buffer, advancing the cursor.
    pub fn parse(buf: &mut impl Buf) -> Result<Self> {
        if buf.remaining() < 4 {
            return Err(ProDjLinkError::DbServer(
                "not enough data for message magic".into(),
            ));
        }
        let magic = buf.get_u32();
        if magic != MESSAGE_START {
            return Err(ProDjLinkError::DbServer(format!(
                "invalid message magic: expected 0x{MESSAGE_START:08x}, got 0x{magic:08x}"
            )));
        }

        if buf.remaining() < 7 {
            return Err(ProDjLinkError::DbServer(
                "not enough data for message header".into(),
            ));
        }
        let transaction = buf.get_u32();
        let kind = MessageType::from(buf.get_u16());
        let arg_count = buf.get_u8() as usize;

        if arg_count > MAX_ARGS {
            return Err(ProDjLinkError::DbServer(format!(
                "too many arguments: {arg_count} exceeds maximum of {MAX_ARGS}"
            )));
        }

        if buf.remaining() < arg_count {
            return Err(ProDjLinkError::DbServer(
                "not enough data for argument type list".into(),
            ));
        }
        // Read (and discard) the type-tag bytes; Field::parse reads its own tag.
        for _ in 0..arg_count {
            let _ = buf.get_u8();
        }

        let mut args = Vec::with_capacity(arg_count);
        for i in 0..arg_count {
            let field = Field::parse(buf).map_err(|e| {
                ProDjLinkError::DbServer(format!("failed to parse argument {i}: {e}"))
            })?;
            args.push(field);
        }

        Ok(Self {
            transaction,
            kind,
            args,
        })
    }

    /// Serialize this message to bytes.
    pub fn serialize(&self) -> BytesMut {
        let mut buf = BytesMut::new();
        buf.put_u32(MESSAGE_START);
        buf.put_u32(self.transaction);
        buf.put_u16(u16::from(self.kind));
        buf.put_u8(self.args.len() as u8);

        // Argument type tags
        for arg in &self.args {
            buf.put_u8(arg.arg_type());
        }

        // Serialized field values
        for arg in &self.args {
            arg.serialize(&mut buf);
        }

        buf
    }

    /// Get argument at index, or error if out of bounds.
    pub fn arg(&self, index: usize) -> Result<&Field> {
        self.args.get(index).ok_or_else(|| {
            ProDjLinkError::DbServer(format!(
                "argument index {index} out of bounds (message has {} args)",
                self.args.len()
            ))
        })
    }

    /// Convenience: get a number argument at index.
    pub fn arg_number(&self, index: usize) -> Result<u32> {
        self.arg(index)?.as_number()
    }

    /// Convenience: get a string argument at index.
    pub fn arg_string(&self, index: usize) -> Result<&str> {
        self.arg(index)?.as_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    /// Build the raw wire bytes for a message by hand.
    fn build_wire_bytes(
        transaction: u32,
        kind: u16,
        fields: &[Field],
    ) -> BytesMut {
        let mut buf = BytesMut::new();
        buf.put_u32(MESSAGE_START);
        buf.put_u32(transaction);
        buf.put_u16(kind);
        buf.put_u8(fields.len() as u8);
        for f in fields {
            buf.put_u8(f.arg_type());
        }
        for f in fields {
            f.serialize(&mut buf);
        }
        buf
    }

    #[test]
    fn parse_hand_crafted_message() {
        let fields = vec![
            Field::number_with_size(1, 4),
            Field::string("hello"),
        ];
        let wire = build_wire_bytes(0x01, 0x2002, &fields);
        let msg = Message::parse(&mut &wire[..]).unwrap();

        assert_eq!(msg.transaction, 0x01);
        assert_eq!(msg.kind, MessageType::MetadataReq);
        assert_eq!(msg.args.len(), 2);
        assert_eq!(msg.arg_number(0).unwrap(), 1);
        assert_eq!(msg.arg_string(1).unwrap(), "hello");
    }

    #[test]
    fn serialize_and_verify_wire_bytes() {
        let msg = Message::new(
            0x07,
            MessageType::SetupReq,
            vec![Field::number_with_size(0x11, 4)],
        );
        let wire = msg.serialize();
        let expected = build_wire_bytes(0x07, 0x0000, &[Field::number_with_size(0x11, 4)]);
        assert_eq!(wire, expected);
    }

    #[test]
    fn round_trip() {
        let original = Message::new(
            42,
            MessageType::AlbumArtReq,
            vec![
                Field::number_with_size(100, 4),
                Field::binary(vec![0xDE, 0xAD]),
                Field::string("track.mp3"),
            ],
        );
        let wire = original.serialize();
        let parsed = Message::parse(&mut &wire[..]).unwrap();

        assert_eq!(parsed.transaction, original.transaction);
        assert_eq!(parsed.kind, original.kind);
        assert_eq!(parsed.args, original.args);
    }

    #[test]
    fn parse_zero_args() {
        let wire = build_wire_bytes(0, 0x4000, &[]);
        let msg = Message::parse(&mut &wire[..]).unwrap();

        assert_eq!(msg.kind, MessageType::MenuAvailable);
        assert!(msg.args.is_empty());
    }

    #[test]
    fn parse_all_three_field_types() {
        let fields = vec![
            Field::number_with_size(0xFF, 1),
            Field::binary(Bytes::from_static(&[1, 2, 3])),
            Field::string("mixed"),
        ];
        let wire = build_wire_bytes(99, 0x3000, &fields);
        let msg = Message::parse(&mut &wire[..]).unwrap();

        assert_eq!(msg.args.len(), 3);
        assert_eq!(msg.args[0].as_number().unwrap(), 0xFF);
        assert_eq!(msg.args[1].as_binary().unwrap().as_ref(), &[1, 2, 3]);
        assert_eq!(msg.args[2].as_string().unwrap(), "mixed");
    }

    #[test]
    fn error_on_invalid_magic() {
        let mut buf = BytesMut::new();
        buf.put_u32(0xDEADBEEF); // wrong magic
        buf.put_u32(0);
        buf.put_u16(0);
        buf.put_u8(0);

        let err = Message::parse(&mut &buf[..]).unwrap_err();
        assert!(err.to_string().contains("invalid message magic"));
    }

    #[test]
    fn error_on_too_many_args() {
        let mut buf = BytesMut::new();
        buf.put_u32(MESSAGE_START);
        buf.put_u32(0);
        buf.put_u16(0);
        buf.put_u8(13); // exceeds MAX_ARGS

        let err = Message::parse(&mut &buf[..]).unwrap_err();
        assert!(err.to_string().contains("too many arguments"));
    }

    #[test]
    fn message_type_round_trip() {
        let known_types: &[(u16, MessageType)] = &[
            (0x0000, MessageType::SetupReq),
            (0x4000, MessageType::MenuAvailable),
            (0x4001, MessageType::MenuHeader),
            (0x4101, MessageType::MenuItem),
            (0x4201, MessageType::MenuFooter),
            (0x2002, MessageType::MetadataReq),
            (0x2003, MessageType::AlbumArtReq),
            (0x2004, MessageType::WaveformPreviewReq),
            (0x2904, MessageType::WaveformDetailReq),
            (0x2104, MessageType::CueListReq),
            (0x2b04, MessageType::CueListExtReq),
            (0x2204, MessageType::BeatGridReq),
            (0x2008, MessageType::AnlzTagReq),
            (0x3000, MessageType::RenderMenuReq),
        ];
        for &(raw, expected) in known_types {
            let mt = MessageType::from(raw);
            assert_eq!(mt, expected);
            assert_eq!(u16::from(mt), raw);
        }

        // Unknown variant round-trips
        let unknown = MessageType::from(0xBEEF);
        assert_eq!(unknown, MessageType::Unknown(0xBEEF));
        assert_eq!(u16::from(unknown), 0xBEEF);
    }

    #[test]
    fn arg_accessor_bounds_checking() {
        let msg = Message::new(1, MessageType::SetupReq, vec![Field::number(5)]);

        assert!(msg.arg(0).is_ok());
        assert!(msg.arg(1).is_err());
        assert!(msg.arg_number(0).is_ok());
        assert!(msg.arg_number(1).is_err());
        assert!(msg.arg_string(0).is_err()); // wrong type
    }
}
