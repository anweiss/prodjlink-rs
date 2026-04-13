//! Mock packet builders and test fixtures for hardware-free testing.
//!
//! This module provides realistic packet construction utilities so that
//! library consumers can write integration tests without connecting real
//! CDJs, mixers, or other Pioneer Pro DJ Link hardware.
//!
//! # Modules
//!
//! - [`packets`] — Builder functions and builder structs for every packet type.
//! - [`fixtures`] — Pre-built "golden" packets representing common real-world devices.
//! - [`scenarios`] — Multi-packet sequences simulating network conversations.

pub mod fixtures;
pub mod packets;
pub mod scenarios;

#[cfg(test)]
mod tests;
