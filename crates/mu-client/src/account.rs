//! La cuenta y sus personajes (la lista llega del servidor, `ReceiveCharacterList`).
//!
//! Las clases con lo que de ellas necesitan la selección, la creación y el juego: las 6 del
//! Main 5.2 y el Rage Fighter, que ese cliente tiene apagado (`PBG_ADD_NEWCHAR_MONK`) y se saca de
//! MuMain (`MAX_CLASS` = 7).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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

    /// Del índice `CLASS_*` (0 a 6); lo desconocido, Dark Knight.
    pub fn from_index(i: u8) -> Self {
        Self::ALL.get(i as usize).copied().unwrap_or(Class::Knight)
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

/// Un personaje de la cuenta.
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

/// La cuenta que entró (`usuario=` de `mu-launcher.txt`).
pub fn current_account() -> String {
    crate::settings::saved_value("usuario").filter(|n| !n.is_empty()).unwrap_or_else(|| "mu-rs".to_string())
}


/// Lo que manda el servidor al entrar con el personaje (`CharacterInformation`): mapa, casilla,
/// estadísticas, vida, maná...
#[derive(Resource, Clone, Debug)]
pub struct HeroInfo(pub mu_protocol::game::CharacterInformation);

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
