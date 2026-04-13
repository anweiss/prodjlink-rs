use thiserror::Error;

/// Errors that can occur in the Pro DJ Link protocol.
#[derive(Debug, Error)]
pub enum ProDjLinkError {
    /// I/O error from network or file operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to parse a packet.
    #[error("packet parse error: {0}")]
    Parse(String),

    /// Packet is shorter than the minimum expected length.
    #[error("packet too short: expected {expected} bytes, got {actual}")]
    PacketTooShort { expected: usize, actual: usize },

    /// Packet does not start with the Pro DJ Link magic header.
    #[error("invalid magic header: packet is not a Pro DJ Link message")]
    InvalidMagic,

    /// Unrecognized packet type byte.
    #[error("invalid packet type: 0x{0:02x}")]
    InvalidPacketType(u8),

    /// Error from the dbserver protocol layer.
    #[error("dbserver error: {0}")]
    DbServer(String),

    /// TCP connection to a device failed.
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    /// An operation timed out waiting for a response.
    #[error("operation timed out")]
    Timeout,

    /// The requested device was not found on the network.
    #[error("device #{0} not found on the network")]
    DeviceNotFound(u8),

    /// Device number is outside the valid range (1–127).
    #[error("invalid device number {0}: must be in range 1-127")]
    InvalidDeviceNumber(u8),

    /// The broadcast channel has been closed.
    #[error("broadcast channel closed")]
    ChannelClosed,

    /// The device number claim was rejected by another device on the network.
    #[error("device number {0} is already in use on the network")]
    DeviceNumberInUse(u8),

    /// No available device numbers could be found in the allowed range.
    #[error("no available device numbers found in the allowed range")]
    NoAvailableDeviceNumber,
}

/// Convenience result type for Pro DJ Link operations.
pub type Result<T> = std::result::Result<T, ProDjLinkError>;
