use bytes::{Buf, BufMut, Bytes};
#[cfg(test)]
use bytes::BytesMut;

use crate::error::{ProDjLinkError, Result};

// Argument type tags used in message argument type lists.
pub const ARG_TYPE_STRING: u8 = 0x02;
pub const ARG_TYPE_BINARY: u8 = 0x03;
pub const ARG_TYPE_NUMBER: u8 = 0x06;

// Wire type tags that prefix each field value.
pub const WIRE_TYPE_NUMBER_1: u8 = 0x0f;
pub const WIRE_TYPE_NUMBER_2: u8 = 0x10;
pub const WIRE_TYPE_NUMBER_4: u8 = 0x11;
pub const WIRE_TYPE_BINARY: u8 = 0x14;
pub const WIRE_TYPE_STRING: u8 = 0x26;

/// A typed field in a dbserver message.
#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    /// A numeric value (1, 2, or 4 bytes on the wire).
    Number {
        value: u32,
        /// Wire size: 1, 2, or 4 bytes.
        size: u8,
    },
    /// A binary blob.
    Binary { data: Bytes },
    /// A text string (stored as Rust String, encoded as UTF-16BE on wire).
    String { text: String },
}

impl Field {
    /// Create a number field with automatic size selection based on value.
    pub fn number(value: u32) -> Self {
        let size = if value <= 0xFF {
            1
        } else if value <= 0xFFFF {
            2
        } else {
            4
        };
        Field::Number { value, size }
    }

    /// Create a number field with explicit wire size (1, 2, or 4).
    pub fn number_with_size(value: u32, size: u8) -> Self {
        Field::Number { value, size }
    }

    /// Create a binary field.
    pub fn binary(data: impl Into<Bytes>) -> Self {
        Field::Binary { data: data.into() }
    }

    /// Create a string field.
    pub fn string(text: impl Into<String>) -> Self {
        Field::String { text: text.into() }
    }

    /// The argument type tag for this field.
    pub fn arg_type(&self) -> u8 {
        match self {
            Field::Number { .. } => ARG_TYPE_NUMBER,
            Field::Binary { .. } => ARG_TYPE_BINARY,
            Field::String { .. } => ARG_TYPE_STRING,
        }
    }

    /// Parse a field from a byte buffer, advancing the cursor.
    pub fn parse(buf: &mut impl Buf) -> Result<Self> {
        if buf.remaining() < 1 {
            return Err(ProDjLinkError::Parse(
                "not enough data for field type tag".into(),
            ));
        }
        let tag = buf.get_u8();
        match tag {
            WIRE_TYPE_NUMBER_1 => {
                if buf.remaining() < 1 {
                    return Err(ProDjLinkError::Parse(
                        "not enough data for 1-byte number field".into(),
                    ));
                }
                let value = buf.get_u8() as u32;
                Ok(Field::Number { value, size: 1 })
            }
            WIRE_TYPE_NUMBER_2 => {
                if buf.remaining() < 2 {
                    return Err(ProDjLinkError::Parse(
                        "not enough data for 2-byte number field".into(),
                    ));
                }
                let value = buf.get_u16() as u32;
                Ok(Field::Number { value, size: 2 })
            }
            WIRE_TYPE_NUMBER_4 => {
                if buf.remaining() < 4 {
                    return Err(ProDjLinkError::Parse(
                        "not enough data for 4-byte number field".into(),
                    ));
                }
                let value = buf.get_u32();
                Ok(Field::Number { value, size: 4 })
            }
            WIRE_TYPE_BINARY => {
                if buf.remaining() < 4 {
                    return Err(ProDjLinkError::Parse(
                        "not enough data for binary field length".into(),
                    ));
                }
                let byte_len = buf.get_u32() as usize;
                if buf.remaining() < byte_len {
                    return Err(ProDjLinkError::Parse(format!(
                        "not enough data for binary field: need {byte_len}, have {}",
                        buf.remaining()
                    )));
                }
                let data = buf.copy_to_bytes(byte_len);
                Ok(Field::Binary { data })
            }
            WIRE_TYPE_STRING => {
                if buf.remaining() < 4 {
                    return Err(ProDjLinkError::Parse(
                        "not enough data for string field length".into(),
                    ));
                }
                let char_count = buf.get_u32() as usize;
                let byte_len = char_count * 2;
                if buf.remaining() < byte_len {
                    return Err(ProDjLinkError::Parse(format!(
                        "not enough data for string field: need {byte_len}, have {}",
                        buf.remaining()
                    )));
                }
                // Read UTF-16BE code units
                let mut code_units = Vec::with_capacity(char_count);
                for _ in 0..char_count {
                    code_units.push(buf.get_u16());
                }
                // Strip trailing null terminator if present
                if code_units.last() == Some(&0) {
                    code_units.pop();
                }
                let text = String::from_utf16(&code_units).map_err(|e| {
                    ProDjLinkError::Parse(format!("invalid UTF-16 in string field: {e}"))
                })?;
                Ok(Field::String { text })
            }
            _ => Err(ProDjLinkError::Parse(format!(
                "unknown field type tag: 0x{tag:02x}"
            ))),
        }
    }

    /// Serialize this field to a byte buffer.
    pub fn serialize(&self, buf: &mut impl BufMut) {
        match self {
            Field::Number { value, size } => match size {
                1 => {
                    buf.put_u8(WIRE_TYPE_NUMBER_1);
                    buf.put_u8(*value as u8);
                }
                2 => {
                    buf.put_u8(WIRE_TYPE_NUMBER_2);
                    buf.put_u16(*value as u16);
                }
                _ => {
                    buf.put_u8(WIRE_TYPE_NUMBER_4);
                    buf.put_u32(*value);
                }
            },
            Field::Binary { data } => {
                buf.put_u8(WIRE_TYPE_BINARY);
                buf.put_u32(data.len() as u32);
                buf.put_slice(data);
            }
            Field::String { text } => {
                buf.put_u8(WIRE_TYPE_STRING);
                let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                buf.put_u32(utf16.len() as u32);
                for unit in &utf16 {
                    buf.put_u16(*unit);
                }
            }
        }
    }

    /// Get the numeric value, or error if not a Number.
    pub fn as_number(&self) -> Result<u32> {
        match self {
            Field::Number { value, .. } => Ok(*value),
            _ => Err(ProDjLinkError::Parse(
                "expected Number field".into(),
            )),
        }
    }

    /// Get the binary data, or error if not Binary.
    pub fn as_binary(&self) -> Result<&Bytes> {
        match self {
            Field::Binary { data } => Ok(data),
            _ => Err(ProDjLinkError::Parse(
                "expected Binary field".into(),
            )),
        }
    }

    /// Get the string text, or error if not String.
    pub fn as_string(&self) -> Result<&str> {
        match self {
            Field::String { text } => Ok(text.as_str()),
            _ => Err(ProDjLinkError::Parse(
                "expected String field".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Number field tests ---

    #[test]
    fn parse_number_1_byte() {
        let data: &[u8] = &[WIRE_TYPE_NUMBER_1, 0x42];
        let field = Field::parse(&mut &data[..]).unwrap();
        assert_eq!(field, Field::Number { value: 0x42, size: 1 });
    }

    #[test]
    fn parse_number_2_byte() {
        let data: &[u8] = &[WIRE_TYPE_NUMBER_2, 0x01, 0x23];
        let field = Field::parse(&mut &data[..]).unwrap();
        assert_eq!(field, Field::Number { value: 0x0123, size: 2 });
    }

    #[test]
    fn parse_number_4_byte() {
        let data: &[u8] = &[WIRE_TYPE_NUMBER_4, 0xDE, 0xAD, 0xBE, 0xEF];
        let field = Field::parse(&mut &data[..]).unwrap();
        assert_eq!(field, Field::Number { value: 0xDEADBEEF, size: 4 });
    }

    #[test]
    fn serialize_number_1_byte() {
        let field = Field::number_with_size(0x42, 1);
        let mut buf = BytesMut::new();
        field.serialize(&mut buf);
        assert_eq!(&buf[..], &[WIRE_TYPE_NUMBER_1, 0x42]);
    }

    #[test]
    fn serialize_number_2_byte() {
        let field = Field::number_with_size(0x0123, 2);
        let mut buf = BytesMut::new();
        field.serialize(&mut buf);
        assert_eq!(&buf[..], &[WIRE_TYPE_NUMBER_2, 0x01, 0x23]);
    }

    #[test]
    fn serialize_number_4_byte() {
        let field = Field::number_with_size(0xDEADBEEF, 4);
        let mut buf = BytesMut::new();
        field.serialize(&mut buf);
        assert_eq!(&buf[..], &[WIRE_TYPE_NUMBER_4, 0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn round_trip_number() {
        for (value, size) in [(0u32, 1), (255, 1), (256, 2), (65535, 2), (65536, 4), (0xDEADBEEF, 4)] {
            let original = Field::number_with_size(value, size);
            let mut buf = BytesMut::new();
            original.serialize(&mut buf);
            let parsed = Field::parse(&mut &buf[..]).unwrap();
            assert_eq!(original, parsed, "round-trip failed for value={value}, size={size}");
        }
    }

    #[test]
    fn auto_size_selection() {
        assert_eq!(Field::number(0), Field::Number { value: 0, size: 1 });
        assert_eq!(Field::number(255), Field::Number { value: 255, size: 1 });
        assert_eq!(Field::number(256), Field::Number { value: 256, size: 2 });
        assert_eq!(Field::number(65535), Field::Number { value: 65535, size: 2 });
        assert_eq!(Field::number(65536), Field::Number { value: 65536, size: 4 });
        assert_eq!(Field::number(0xFFFFFFFF), Field::Number { value: 0xFFFFFFFF, size: 4 });
    }

    // --- Binary field tests ---

    #[test]
    fn parse_binary() {
        let data: &[u8] = &[WIRE_TYPE_BINARY, 0x00, 0x00, 0x00, 0x03, 0xAA, 0xBB, 0xCC];
        let field = Field::parse(&mut &data[..]).unwrap();
        assert_eq!(field, Field::Binary { data: Bytes::from_static(&[0xAA, 0xBB, 0xCC]) });
    }

    #[test]
    fn parse_binary_empty() {
        let data: &[u8] = &[WIRE_TYPE_BINARY, 0x00, 0x00, 0x00, 0x00];
        let field = Field::parse(&mut &data[..]).unwrap();
        assert_eq!(field, Field::Binary { data: Bytes::new() });
    }

    #[test]
    fn serialize_binary() {
        let field = Field::binary(vec![0xAA, 0xBB, 0xCC]);
        let mut buf = BytesMut::new();
        field.serialize(&mut buf);
        assert_eq!(&buf[..], &[WIRE_TYPE_BINARY, 0x00, 0x00, 0x00, 0x03, 0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn serialize_binary_empty() {
        let field = Field::binary(Bytes::new());
        let mut buf = BytesMut::new();
        field.serialize(&mut buf);
        assert_eq!(&buf[..], &[WIRE_TYPE_BINARY, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn round_trip_binary() {
        let original = Field::binary(vec![1, 2, 3, 4, 5]);
        let mut buf = BytesMut::new();
        original.serialize(&mut buf);
        let parsed = Field::parse(&mut &buf[..]).unwrap();
        assert_eq!(original, parsed);
    }

    // --- String field tests ---

    #[test]
    fn parse_string_ascii() {
        // "Hi" + null terminator = 3 chars, 6 bytes of UTF-16BE
        let mut data = vec![WIRE_TYPE_STRING, 0x00, 0x00, 0x00, 0x03];
        data.extend_from_slice(&[0x00, b'H', 0x00, b'i', 0x00, 0x00]);
        let field = Field::parse(&mut &data[..]).unwrap();
        assert_eq!(field, Field::String { text: "Hi".into() });
    }

    #[test]
    fn serialize_string_ascii() {
        let field = Field::string("Hi");
        let mut buf = BytesMut::new();
        field.serialize(&mut buf);
        // 3 chars (H, i, \0), each 2 bytes
        let expected: &[u8] = &[
            WIRE_TYPE_STRING,
            0x00, 0x00, 0x00, 0x03, // char count
            0x00, b'H', 0x00, b'i', 0x00, 0x00, // UTF-16BE with null terminator
        ];
        assert_eq!(&buf[..], expected);
    }

    #[test]
    fn round_trip_string_ascii() {
        let original = Field::string("Hello, World!");
        let mut buf = BytesMut::new();
        original.serialize(&mut buf);
        let parsed = Field::parse(&mut &buf[..]).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn round_trip_string_japanese() {
        let original = Field::string("こんにちは");
        let mut buf = BytesMut::new();
        original.serialize(&mut buf);
        let parsed = Field::parse(&mut &buf[..]).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn string_non_ascii_wire_format() {
        // Verify Japanese "あ" (U+3042) encodes correctly
        let field = Field::string("あ");
        let mut buf = BytesMut::new();
        field.serialize(&mut buf);
        // 2 chars: 'あ' + null
        let expected: &[u8] = &[
            WIRE_TYPE_STRING,
            0x00, 0x00, 0x00, 0x02, // char count = 2
            0x30, 0x42, // U+3042 in UTF-16BE
            0x00, 0x00, // null terminator
        ];
        assert_eq!(&buf[..], expected);
    }

    #[test]
    fn round_trip_string_empty() {
        let original = Field::string("");
        let mut buf = BytesMut::new();
        original.serialize(&mut buf);
        let parsed = Field::parse(&mut &buf[..]).unwrap();
        assert_eq!(original, parsed);
    }

    // --- Error cases ---

    #[test]
    fn parse_unknown_type_tag() {
        let data: &[u8] = &[0xFF, 0x00];
        let err = Field::parse(&mut &data[..]).unwrap_err();
        assert!(err.to_string().contains("unknown field type tag"));
    }

    #[test]
    fn parse_empty_buffer() {
        let data: &[u8] = &[];
        let err = Field::parse(&mut &data[..]).unwrap_err();
        assert!(err.to_string().contains("not enough data"));
    }

    #[test]
    fn parse_truncated_number() {
        let data: &[u8] = &[WIRE_TYPE_NUMBER_4, 0x01, 0x02]; // need 4 bytes, only 2
        let err = Field::parse(&mut &data[..]).unwrap_err();
        assert!(err.to_string().contains("not enough data"));
    }

    #[test]
    fn parse_truncated_binary() {
        let data: &[u8] = &[WIRE_TYPE_BINARY, 0x00, 0x00, 0x00, 0x05, 0x01, 0x02]; // need 5, only 2
        let err = Field::parse(&mut &data[..]).unwrap_err();
        assert!(err.to_string().contains("not enough data"));
    }

    // --- Accessor tests ---

    #[test]
    fn as_number_success() {
        let field = Field::number(42);
        assert_eq!(field.as_number().unwrap(), 42);
    }

    #[test]
    fn as_number_error_on_binary() {
        let field = Field::binary(vec![1, 2, 3]);
        assert!(field.as_number().is_err());
    }

    #[test]
    fn as_number_error_on_string() {
        let field = Field::string("hello");
        assert!(field.as_number().is_err());
    }

    #[test]
    fn as_binary_success() {
        let field = Field::binary(vec![1, 2, 3]);
        assert_eq!(field.as_binary().unwrap().as_ref(), &[1, 2, 3]);
    }

    #[test]
    fn as_binary_error_on_number() {
        let field = Field::number(42);
        assert!(field.as_binary().is_err());
    }

    #[test]
    fn as_string_success() {
        let field = Field::string("hello");
        assert_eq!(field.as_string().unwrap(), "hello");
    }

    #[test]
    fn as_string_error_on_number() {
        let field = Field::number(42);
        assert!(field.as_string().is_err());
    }

    // --- Arg type tags ---

    #[test]
    fn arg_type_tags() {
        assert_eq!(Field::number(0).arg_type(), ARG_TYPE_NUMBER);
        assert_eq!(Field::binary(vec![]).arg_type(), ARG_TYPE_BINARY);
        assert_eq!(Field::string("").arg_type(), ARG_TYPE_STRING);
    }
}
