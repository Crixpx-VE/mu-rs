//! MU Online network protocol (Season 6 Episode 3, OpenMU-compatible).
//!
//! Used by both client and server, so a change here affects both.
//!
//! Reference: `referencias/OpenMU/src/Network/Packets/**/*.xml` (MIT license).

pub mod appearance;
pub mod connect;
pub mod crypto;
pub mod game;
pub mod packet;
pub mod simple_modulus;
pub mod xor;

pub use packet::{PacketHeader, packet_len};
