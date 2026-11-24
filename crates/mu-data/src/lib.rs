//! Readers for the MU Online (Season 6) client file formats.
//!
//! Does not depend on any graphics engine: everything returns raw data
//! (RGBA bytes, heights, indices), and the client decides how to draw it.
//!
//! References:
//! - `referencias/xulek-muonline/Client.Data` (C#)
//! - `referencias/muonlinejs` (TypeScript, licencia MIT)

pub mod bmd;
pub mod crypt;
pub mod fs;
pub mod items;
pub mod minimap;
pub mod monsters;
pub mod objects;
pub mod path;
pub mod reader;
pub mod terrain;
pub mod texture;
pub mod warps;

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    Io(PathBuf, std::io::Error),
    NotFound(PathBuf),
    Format(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(p, e) => write!(f, "{}: {e}", p.display()),
            Error::NotFound(p) => write!(f, "no existe: {}", p.display()),
            Error::Format(m) => write!(f, "formato inválido: {m}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
