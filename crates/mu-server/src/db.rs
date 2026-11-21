//! Cuentas y personajes en un archivo JSON (`cuentas.json` en la carpeta de datos del servidor).
//!
//! De momento basta: pocos jugadores y todo en memoria; se escribe entero en cada cambio (primero
//! a un archivo temporal y luego se renombra, para no dejarlo a medias). Las contraseñas se guardan
//! como SHA-256 de `cuenta:contraseña`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::classes::{self, ClassIndex, Stats};

/// Huecos de personaje por cuenta.
pub const MAX_SLOTS: u8 = 5;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    pub slot: u8,
    pub class: ClassIndex,
    pub level: u16,
    pub experience: u64,
    pub level_up_points: u16,
    pub stats: Stats,
    pub money: u32,
    pub health: u16,
    pub mana: u16,
    pub map: u8,
    pub x: u8,
    pub y: u8,
    pub direction: u8,
    /// `GuildStatus` (255 = sin gremio).
    #[serde(default = "no_guild")]
    pub guild_status: u8,
}

fn no_guild() -> u8 {
    255
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub password_hash: String,
    #[serde(default)]
    pub characters: Vec<Character>,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    accounts: BTreeMap<String, Account>,
}

pub struct Db {
    path: PathBuf,
    store: Store,
}

pub fn hash_password(account: &str, password: &str) -> String {
    let digest = Sha256::digest(format!("{account}:{password}").as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Por qué no se pudo crear un personaje.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateError {
    /// Nombre inválido o ya usado.
    BadName,
    /// No queda hueco.
    Full,
}

/// `CheckName` y `CheckSpecialText` del cliente: de 4 a 10 letras o números ASCII, sin los nombres
/// de la casa.
pub fn valid_name(name: &str) -> bool {
    (4..=10).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric())
        && !["webzen", "operation"].iter().any(|w| name.to_ascii_lowercase().contains(w))
}

impl Db {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let store = match std::fs::read_to_string(path) {
            Ok(t) => serde_json::from_str(&t).map_err(std::io::Error::other)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Store::default(),
            Err(e) => return Err(e),
        };
        Ok(Self { path: path.to_path_buf(), store })
    }

    fn save(&self) {
        let tmp = self.path.with_extension("json.tmp");
        let result = serde_json::to_string_pretty(&self.store)
            .map_err(std::io::Error::other)
            .and_then(|t| std::fs::write(&tmp, t))
            .and_then(|_| std::fs::rename(&tmp, &self.path));
        if let Err(e) = result {
            eprintln!("[db] no se pudo guardar {}: {e}", self.path.display());
        }
    }

    pub fn account(&self, name: &str) -> Option<&Account> {
        self.store.accounts.get(name)
    }

    /// Crea la cuenta (cuando entra por primera vez).
    pub fn create_account(&mut self, name: &str, password: &str) {
        self.store
            .accounts
            .insert(name.to_string(), Account { password_hash: hash_password(name, password), characters: Vec::new() });
        self.save();
    }

    pub fn check_password(&self, account: &str, password: &str) -> bool {
        self.account(account).is_some_and(|a| a.password_hash == hash_password(account, password))
    }

    pub fn characters(&self, account: &str) -> Vec<Character> {
        let mut list = self.account(account).map(|a| a.characters.clone()).unwrap_or_default();
        list.sort_by_key(|c| c.slot);
        list
    }

    pub fn character(&self, account: &str, name: &str) -> Option<Character> {
        self.account(account)?.characters.iter().find(|c| c.name == name).cloned()
    }

    fn name_taken(&self, name: &str) -> bool {
        self.store.accounts.values().flat_map(|a| &a.characters).any(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// Un personaje nuevo de nivel 1 en el primer hueco libre, en `spawn` de su mapa de inicio.
    pub fn create_character(
        &mut self,
        account: &str,
        name: &str,
        class: ClassIndex,
        spawn: impl FnOnce(u8) -> (u8, u8),
    ) -> Result<Character, CreateError> {
        if !valid_name(name) || class >= classes::MAX_CLASS || self.name_taken(name) {
            return Err(CreateError::BadName);
        }
        let info = classes::info(class);
        let (x, y) = spawn(info.home_map);
        let acc = self.store.accounts.get_mut(account).ok_or(CreateError::BadName)?;
        let slot = (0..MAX_SLOTS).find(|s| !acc.characters.iter().any(|c| c.slot == *s)).ok_or(CreateError::Full)?;
        let c = Character {
            name: name.to_string(),
            slot,
            class,
            level: 1,
            experience: 0,
            level_up_points: 0,
            stats: info.stats,
            money: 0,
            health: info.health,
            mana: info.mana,
            map: info.home_map,
            x,
            y,
            direction: 0,
            guild_status: 255,
        };
        acc.characters.push(c.clone());
        self.save();
        Ok(c)
    }

    pub fn delete_character(&mut self, account: &str, name: &str) -> bool {
        let Some(acc) = self.store.accounts.get_mut(account) else { return false };
        let before = acc.characters.len();
        acc.characters.retain(|c| c.name != name);
        let deleted = acc.characters.len() != before;
        if deleted {
            self.save();
        }
        deleted
    }

    /// Guarda dónde está un personaje (al salir del mundo).
    pub fn update_position(&mut self, account: &str, name: &str, map: u8, x: u8, y: u8, direction: u8) {
        let Some(c) = self.store.accounts.get_mut(account).and_then(|a| a.characters.iter_mut().find(|c| c.name == name))
        else {
            return;
        };
        (c.map, c.x, c.y, c.direction) = (map, x, y, direction);
        self.save();
    }

    /// Guarda el zen de un personaje (al pagar un viaje).
    pub fn set_money(&mut self, account: &str, name: &str, money: u32) {
        let Some(c) = self.store.accounts.get_mut(account).and_then(|a| a.characters.iter_mut().find(|c| c.name == name))
        else {
            return;
        };
        c.money = money;
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres() {
        assert!(valid_name("crixpx"));
        assert!(!valid_name("abc"));
        assert!(!valid_name("con espacio"));
        assert!(!valid_name("MiWebzen1"));
    }

    #[test]
    fn crear_y_borrar() {
        let dir = std::env::temp_dir().join(format!("mu-server-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cuentas.json");
        let _ = std::fs::remove_file(&path);
        let mut db = Db::open(&path).unwrap();
        db.create_account("pepe", "secreto");
        assert!(db.check_password("pepe", "secreto"));
        assert!(!db.check_password("pepe", "otro"));
        let c = db.create_character("pepe", "Guerrero", 1, |_| (130, 130)).unwrap();
        assert_eq!(c.slot, 0);
        assert_eq!(db.create_character("pepe", "guerrero", 0, |_| (130, 130)).unwrap_err(), CreateError::BadName);
        // Se guarda y se vuelve a leer.
        let db2 = Db::open(&path).unwrap();
        assert_eq!(db2.characters("pepe").len(), 1);
        let mut db = db2;
        assert!(db.delete_character("pepe", "Guerrero"));
        assert!(db.characters("pepe").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
