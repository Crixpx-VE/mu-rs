//! Protocolo de red de MU Online (Season 6 Episode 3, compatible con OpenMU).
//!
//! Lo usan tanto el cliente como el servidor, así que un cambio aquí afecta a los dos.
//!
//! Referencia: `referencias/OpenMU/src/Network/Packets/**/*.xml` (licencia MIT).

pub mod connect;
pub mod crypto;
pub mod game;
pub mod packet;
pub mod simple_modulus;
pub mod xor;

pub use packet::{PacketHeader, packet_len};
