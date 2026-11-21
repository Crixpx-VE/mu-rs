//! Personajes de la cuenta sin servidor: se guardan en `mu-personajes.json`, junto al ejecutable
//! (como `mu-launcher.txt`, es de cada jugador y las actualizaciones no lo tocan). Cuando haya
//! servidor, la lista llegará de él (`ReceiveCharacterList`) y esto sobrará.
//!
//! También las clases con lo que de ellas necesitan la selección, la creación y el juego: las 6 del
//! Main 5.2 y el Rage Fighter, que ese cliente tiene apagado (`PBG_ADD_NEWCHAR_MONK`) y se saca de
//! MuMain (`MAX_CLASS` = 7).

use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Huecos de la cuenta.
pub const MAX_SLOTS: usize = 5;
/// `MAX_ID_SIZE`: largo máximo del nombre.
pub const MAX_NAME: usize = 10;

/// `CLASS_*` del original (el número es el que va en el paquete y en `NewFaceNN`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash)]
pub enum Class {
    Wizard = 0,
    Knight = 1,
    Elf = 2,
    Gladiator = 3,
    Lord = 4,
    Summoner = 5,
    /// `CLASS_RAGEFIGHTER` (de MuMain).
    Fighter = 6,
}

impl Class {
    pub const ALL: [Class; 7] =
        [Class::Wizard, Class::Knight, Class::Elf, Class::Gladiator, Class::Lord, Class::Summoner, Class::Fighter];

    pub fn index(self) -> usize {
        self as usize
    }

    /// `GetCharacterClassText` para la clase base (`GlobalText` 20-24, 1687 y 3150).
    pub fn name(self) -> &'static str {
        match self {
            Class::Wizard => "Dark Wizard",
            Class::Knight => "Dark Knight",
            Class::Elf => "Elf",
            Class::Gladiator => "Magic Gladiator",
            Class::Lord => "Dark Lord",
            Class::Summoner => "Summoner",
            Class::Fighter => "Rage Fighter",
        }
    }

    /// `SetCharacterScale` en el juego (`Skin` 0; MuMain ZzzCharacter.cpp:12924).
    pub fn game_scale(self) -> f32 {
        match self {
            Class::Wizard | Class::Knight | Class::Summoner => 0.9,
            Class::Elf => 0.88,
            Class::Gladiator => 0.95,
            Class::Lord => 0.92,
            Class::Fighter => 1.03,
        }
    }

    /// `SetCharacterScale` en la selección: 1.2, el Rage Fighter 1.35.
    pub fn select_scale(self) -> f32 {
        if self == Class::Fighter { 1.35 } else { 1.2 }
    }

    /// `IsFemale`.
    pub fn female(self) -> bool {
        matches!(self, Class::Elf | Class::Summoner)
    }

    /// Número de las piezas sin equipo (`HelmClassNN.bmd`...): `GetSkinModelIndex` + 1.
    pub fn skin_file(self) -> usize {
        self.index() + 1
    }

    /// Fuerza, agilidad, vitalidad, energía y (el Dark Lord) mando al crear
    /// (`apszStat` de `CCharMakeWin::RenderControls`).
    pub fn base_stats(self) -> (&'static [&'static str; 4], Option<&'static str>) {
        match self {
            Class::Wizard => (&["18", "18", "15", "30"], None),
            Class::Knight => (&["28", "20", "25", "10"], None),
            Class::Elf => (&["22", "25", "20", "15"], None),
            Class::Gladiator => (&["26", "26", "26", "26"], None),
            Class::Lord => (&["26", "20", "20", "15"], Some("25")),
            Class::Summoner => (&["21", "21", "18", "23"], None),
            Class::Fighter => (&["32", "27", "25", "20"], None),
        }
    }
}

/// Un personaje guardado.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoredCharacter {
    pub slot: usize,
    pub name: String,
    pub class: Class,
    pub level: u32,
    /// `GuildStatus` (255 = sin gremio).
    #[serde(default = "no_guild")]
    pub guild_status: u8,
}

fn no_guild() -> u8 {
    255
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    /// Cuenta → sus personajes.
    #[serde(default)]
    accounts: std::collections::BTreeMap<String, Vec<StoredCharacter>>,
}

fn store_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("mu-personajes.json"))
}

fn read_store() -> Store {
    store_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| warn!("mu-personajes.json: {e}")).ok())
        .unwrap_or_default()
}

fn write_store(store: &Store) {
    let Some(path) = store_path() else { return };
    match serde_json::to_string_pretty(store) {
        Ok(t) => {
            if let Err(e) = std::fs::write(&path, t) {
                warn!("no se pudo guardar {}: {e}", path.display());
            }
        }
        Err(e) => warn!("mu-personajes.json: {e}"),
    }
}

/// La cuenta que entró (`usuario=` de `mu-launcher.txt`).
pub fn current_account() -> String {
    crate::settings::saved_value("usuario").filter(|n| !n.is_empty()).unwrap_or_else(|| "mu-rs".to_string())
}

/// Personajes de la cuenta, por hueco.
pub fn characters(account: &str) -> Vec<StoredCharacter> {
    let mut list = read_store().accounts.get(account).cloned().unwrap_or_default();
    list.retain(|c| c.slot < MAX_SLOTS);
    list.sort_by_key(|c| c.slot);
    list
}

/// Por qué no se pudo crear (lo que contestaría el servidor).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CreateError {
    /// El nombre ya lo tiene otro personaje (de cualquier cuenta).
    NameTaken,
    /// No queda hueco.
    Full,
}

/// `SendRequestCreateCharacter`: en el primer hueco libre. Devuelve el personaje creado.
pub fn create(account: &str, name: &str, class: Class) -> Result<StoredCharacter, CreateError> {
    let mut store = read_store();
    let taken = store.accounts.values().flatten().any(|c| c.name.eq_ignore_ascii_case(name));
    if taken {
        return Err(CreateError::NameTaken);
    }
    let list = store.accounts.entry(account.to_string()).or_default();
    let Some(slot) = (0..MAX_SLOTS).find(|s| !list.iter().any(|c| c.slot == *s)) else {
        return Err(CreateError::Full);
    };
    let c = StoredCharacter { slot, name: name.to_string(), class, level: 1, guild_status: 255 };
    list.push(c.clone());
    write_store(&store);
    Ok(c)
}

/// `SendRequestDeleteCharacter`.
pub fn delete(account: &str, name: &str) {
    let mut store = read_store();
    if let Some(list) = store.accounts.get_mut(account) {
        list.retain(|c| c.name != name);
    }
    write_store(&store);
}

/// El personaje con el que se entra al juego (`Hero`): lo pone la selección y lo lee el juego.
/// Sin él (`--world`), un Dark Knight.
#[derive(Resource, Clone, Debug)]
pub struct Hero {
    pub name: String,
    pub class: Class,
    pub level: u32,
}

impl Default for Hero {
    fn default() -> Self {
        Self { name: current_account(), class: Class::Knight, level: 1 }
    }
}
