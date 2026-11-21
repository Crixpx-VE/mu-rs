//! Paquetes del Game Server: entrada, login, selección de personaje y mundo básico
//! (aparecer en el mapa, caminar, ver a otros jugadores y salir).
//!
//! Los índices son absolutos (cuentan la cabecera), igual que en los XML de OpenMU.
//!
//! ```text
//! servidor → cliente  GameServerEntered            C1 0C F1 00 <ok u8> <id jugador u16 BE> <versión 5>
//! cliente  → servidor Login                        C3 3C F1 01 <usuario 10, Xor3> <contraseña 20, Xor3>
//!                                                              <tick u32 BE> <versión 5> <serial 16>
//! servidor → cliente  LoginResponse                C1 05 F1 01 <resultado>
//! cliente  → servidor Logout                       C3 05 F1 02 <tipo: 0 cerrar, 1 elegir personaje, 2 elegir servidor>
//! servidor → cliente  LogoutResponse               C3 05 F1 02 <tipo>
//! cliente  → servidor RequestCharacterList         C1 05 F3 00 <idioma>
//! servidor → cliente  CharacterList                C1 len F3 00 <desbloqueos> <movimientos> <n> <baúl ext.>
//!                                                              n × [<slot> <nombre 10> 00 <nivel u16 LE>
//!                                                                   <estado:4 | bloqueo:4> <apariencia 18> <rango gremio>]
//! servidor → cliente  CharacterClassCreationUnlock C1 05 DE 00 <desbloqueos>
//! cliente  → servidor CreateCharacter              C1 0F F3 01 <nombre 10> <clase << 2>
//! servidor → cliente  CharacterCreationSuccessful  C1 2A F3 01 01 <nombre 10> <slot> <nivel u16 LE> <clase << 3>
//!                                                              <estado> <apariencia 18> FF FF FF FF
//! servidor → cliente  CharacterCreationFailed      C1 05 F3 01 00
//! cliente  → servidor DeleteCharacter              C1 22 F3 02 <nombre 10> <código de seguridad 20>
//! servidor → cliente  CharacterDeleteResponse      C1 05 F3 02 <resultado>
//! cliente  → servidor SelectCharacter              C1 0E F3 03 <nombre 10>
//! servidor → cliente  CharacterInformation         C3 48 F3 03 <x> <y> <mapa> <dirección> <exp u64 BE> <exp sig. u64 BE>
//!                                                              <puntos u16 LE> <fuerza> <agilidad> <vitalidad> <energía>
//!                                                              <vida> <vida máx> <maná> <maná máx> <escudo> <escudo máx>
//!                                                              <AG> <AG máx> 00 00 <zen u32 LE> <héroe> <estado>
//!                                                              <frutas> <frutas máx> <mando> <frutas -> <frutas - máx>
//!                                                              <ampliaciones inventario> 00 00 00   (todos los u16 LE)
//! cliente  → servidor ClientReadyAfterMapChange    C1 04 F3 12
//! cliente  → servidor Ping                         C3 0C 0E 00 <tick u32 LE> <vel. ataque u16 LE> <vel. magia u16 LE>
//! cliente  → servidor Walk (WalkRequest)           C1 len D4 <x origen> <y origen> <rotación:4 | pasos:4> <pasos, 2 por byte>
//! servidor → cliente  ObjectWalked                 C1 len D4 <id u16 BE> <x destino> <y destino> <rotación:4 | pasos:4>
//!                                                              <pasos, 2 por byte>
//! servidor → cliente  AddCharactersToScope         C2 len 12 <n> n × [<id u16 BE> <x> <y> <apariencia 18> <nombre 10>
//!                                                                    <x destino> <y destino> <rotación:4 | héroe:4>
//!                                                                    <n efectos> <efectos n>]
//! servidor → cliente  MapObjectOutOfScope          C1 len 14 <n> n × <id u16 BE>
//! servidor → cliente  MapMoveChecksumKey           C1 08 8E 01 <clave u32 LE>
//! cliente  → servidor WarpCommandRequest           C1 0A 8E 02 <clave u32 LE> <índice u16 LE>
//! servidor → cliente  MapChanged                   C3 0F 1C 0F <cambio de mapa> <mapa u16 BE> <x> <y>
//!                                                              <rotación> 00 00 00 00 00
//! servidor → cliente  InventoryMoneyUpdate         C3 08 22 FE <zen u32 BE>
//! ```
//!
//! # Mover de mapa (`M`)
//!
//! Al entrar, el servidor manda la clave (`ReceiveMoveMapChecksum`, WSclient.cpp:11932). Cada
//! `SendRequestMoveMap` (wsclientinline.h:1217) lleva la siguiente: `GetMoveCommandKey` la pasa por
//! [`map_move_key`] y se la queda (NewUIMoveCommandWindow.cpp:273-278); el servidor hace lo mismo
//! con la suya (`CheckKeyValue`). Si le vale, quita el zen (`InventoryMoneyUpdate`), saca al
//! personaje de la vista de los del mapa y manda `MapChanged`; el cliente carga el mapa y contesta
//! `ClientReadyAfterMapChange` (`SendRequestFinishLoading`, WSclient.cpp:1814), y entonces el
//! servidor lo mete en la vista de los del mapa nuevo (como `PlayerMapTransitions` de OpenMU).
//!
//! `MapChanged` lo lee `ReceiveTeleport` (WSclient.cpp:1698) con `PRECEIVE_TELEPORT_POSITION`
//! (WSclient.h, sin `pack`): `WORD Flag` en el byte 4 (el 5 es el byte alto del mapa de OpenMU),
//! `Map` en el 6, `PositionX/Y` en el 7 y 8 y `Angle` en el 9. `Flag` 0 es un teletransporte en el
//! mismo mapa (sin borrar a los demás); otro valor, cambio de mapa. OpenMU (`MapChanged`) mide 15.
//!
//! Formatos sacados de `OpenMU/src/Network/Packets/*.xml` (licencia MIT), variante Season 6 Ep3,
//! contrastados con el cliente original (`Main5.2`: `WSclient.cpp`, `wsclientinline.h`, `ProtocolSend.cpp`).
//!
//! # Clases en el cable
//!
//! Las clases se guardan como `CharacterClassNumber` de OpenMU (`clase base * 4 + evolución`:
//! DW 0, SM 2, GM 3, DK 4, BK 6, BM 7, Elfa 8, ME 10, HE 11, MG 12, DM 13, DL 16, LE 17,
//! Invocadora 20, BS 22, DiM 23, RF 24, FM 25). Según el paquete van desplazadas:
//! - `CreateCharacter`: `clase << 2` (el cliente manda `(índice << 4) + skin`).
//! - `CharacterCreationSuccessful` y el byte 0 de la apariencia: `clase << 3`
//!   (el cliente lo deshace en `ChangeServerClassTypeToClientClassType`).

use crate::packet::{Packet, c1, c2};
use crate::xor::xor3;

/// Versión y serial del cliente Season 6 Ep3 que acepta OpenMU por defecto ("1.04d", GMO).
pub const CLIENT_VERSION: [u8; 5] = *b"10404";
pub const CLIENT_SERIAL: [u8; 16] = *b"k1Pk2jcET48mxL3b";

/// Bit alto del id de objeto en `AddCharactersToScope` (y en el resto de paquetes de "entra en
/// vista"): el servidor lo pone cuando el objeto acaba de aparecer (spawn / teletransporte) y el
/// cliente lo usa para el efecto de aparición. El id real es `id & OBJECT_ID_MASK`.
/// Este módulo NO lo añade ni lo quita: `ScopeCharacter::id` viaja tal cual.
pub const NEW_OBJECT_FLAG: u16 = 0x8000;
/// Máscara para quedarse con el id real de un objeto.
pub const OBJECT_ID_MASK: u16 = 0x7FFF;

/// Máximo de pasos que caben en un paquete de caminar (el contador es un nibble).
pub const MAX_WALK_STEPS: usize = 15;
/// Máximo de personajes en `CharacterList` para que quepa en un C1 (8 + 7 × 34 = 246).
pub const MAX_CHARACTER_LIST: usize = 7;
/// Máximo de ids en un `MapObjectOutOfScope` para que quepa en un C1 (4 + 125 × 2 = 254).
pub const MAX_OUT_OF_SCOPE_IDS: usize = 125;
/// Máximo de personajes en un `AddCharactersToScope` (el contador es un byte).
pub const MAX_SCOPE_CHARACTERS: usize = 255;

/// Valores de `CharacterDeleteResponse::result` (`CharacterDeleteResult` en OpenMU).
pub mod delete_result {
    /// No se pudo borrar (el cliente lo muestra como "está en un gremio").
    pub const UNSUCCESSFUL: u8 = 0;
    /// Borrado.
    pub const SUCCESSFUL: u8 = 1;
    /// Código de seguridad incorrecto.
    pub const WRONG_SECURITY_CODE: u8 = 2;
    /// No está en OpenMU, pero el cliente original lo entiende como "objetos bloqueados".
    pub const ITEM_BLOCK: u8 = 3;
}

/// Valores de `Logout::kind` / `LogoutResponse::kind` (`LogOutType` en OpenMU).
pub mod logout_kind {
    pub const CLOSE_GAME: u8 = 0;
    pub const BACK_TO_CHARACTER_SELECTION: u8 = 1;
    pub const BACK_TO_SERVER_SELECTION: u8 = 2;
}

/// Bits de `CharacterList::unlock_flags` y `CharacterClassCreationUnlock::flags`
/// (`CharacterCreationUnlockFlags` en `CommonEnums.xml` de OpenMU). 0 = nada desbloqueado.
pub mod unlock {
    pub const SUMMONER: u8 = 1;
    pub const DARK_LORD: u8 = 2;
    pub const MAGIC_GLADIATOR: u8 = 4;
    pub const RAGE_FIGHTER: u8 = 8;
}

/// Resultado del login (`LoginResult` en OpenMU).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginResult {
    InvalidPassword,
    Okay,
    AccountInvalid,
    AccountAlreadyConnected,
    ServerIsFull,
    AccountBlocked,
    WrongVersion,
    ConnectionError,
    ConnectionClosed3Fails,
    TemporaryBlocked,
    Other(u8),
}

impl LoginResult {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::InvalidPassword,
            1 => Self::Okay,
            2 => Self::AccountInvalid,
            3 => Self::AccountAlreadyConnected,
            4 => Self::ServerIsFull,
            5 => Self::AccountBlocked,
            6 => Self::WrongVersion,
            7 => Self::ConnectionError,
            8 => Self::ConnectionClosed3Fails,
            14 => Self::TemporaryBlocked,
            o => Self::Other(o),
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Self::InvalidPassword => 0,
            Self::Okay => 1,
            Self::AccountInvalid => 2,
            Self::AccountAlreadyConnected => 3,
            Self::ServerIsFull => 4,
            Self::AccountBlocked => 5,
            Self::WrongVersion => 6,
            Self::ConnectionError => 7,
            Self::ConnectionClosed3Fails => 8,
            Self::TemporaryBlocked => 14,
            Self::Other(o) => o,
        }
    }

    /// Texto para mostrar al jugador.
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidPassword => "Contraseña incorrecta",
            Self::Okay => "Conectado",
            Self::AccountInvalid => "La cuenta no existe",
            Self::AccountAlreadyConnected => "La cuenta ya está conectada",
            Self::ServerIsFull => "El servidor está lleno",
            Self::AccountBlocked => "La cuenta está bloqueada",
            Self::WrongVersion => "Versión del cliente incorrecta",
            Self::ConnectionError => "Error de conexión",
            Self::ConnectionClosed3Fails => "Conexión cerrada tras 3 intentos fallidos",
            Self::TemporaryBlocked => "La cuenta está bloqueada temporalmente",
            Self::Other(_) => "Error desconocido",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Clases, apariencia y direcciones
// ---------------------------------------------------------------------------------------------

/// Índice de clase del cliente (0 DW, 1 DK, 2 Elfa, 3 MG, 4 DL, 5 Invocadora, 6 RF; los
/// `CLASS_*` del cliente original) → `CharacterClassNumber` de OpenMU sin evolución.
pub fn class_number(base_class_index: u8) -> u8 {
    base_class_index << 2
}

/// `CharacterClassNumber` → índice de clase del cliente (0..6), ignorando las evoluciones.
pub fn base_class_index(class_number: u8) -> u8 {
    class_number >> 2
}

/// Bytes de apariencia (18) de un personaje de la clase `class` (`CharacterClassNumber`) sin
/// nada equipado, como los escribe `AppearanceSerializer` de OpenMU (Season 6):
/// byte 0 = `clase << 3` (+ pose, 0 = de pie), armas a `FF`, índices de armadura con todos
/// los bits a 1 ("vacío"), niveles a 0, sin alas y sin mascota (`byte 5 |= 3`).
pub fn appearance_for_class(class: u8) -> [u8; 18] {
    let mut a = [0u8; 18];
    a[0] = (class << 3) & 0xF8;
    // Mano izquierda / derecha: índice FF, grupo con los 3 bits altos a 1.
    a[1] = 0xFF;
    a[12] |= 0xF0;
    a[2] = 0xFF;
    a[13] |= 0xF0;
    // Casco, armadura, pantalones, guantes y botas vacíos (nibble del índice, bit extra en el
    // byte 9 y nibble alto del índice en los bytes 13..15).
    a[3] |= 0xF0;
    a[9] |= 0x80;
    a[13] |= 0x0F;
    a[3] |= 0x0F;
    a[9] |= 0x40;
    a[14] |= 0xF0;
    a[4] |= 0xF0;
    a[9] |= 0x20;
    a[14] |= 0x0F;
    a[4] |= 0x0F;
    a[9] |= 0x10;
    a[15] |= 0xF0;
    a[5] |= 0xF0;
    a[9] |= 0x08;
    a[15] |= 0x0F;
    // Sin mascota.
    a[5] |= 0x03;
    a
}

/// Clase (`CharacterClassNumber`) a partir de los bytes de apariencia (solo mira el byte 0).
pub fn class_from_appearance(appearance: &[u8]) -> u8 {
    appearance.first().map_or(0, |&b| b >> 3)
}

/// Tabla de direcciones de MU: `(dx, dy)` para cada dirección 0..7 de los paquetes de caminar.
///
/// Es `DirTable` de `WSclient.cpp` del cliente original, que usa `SendCharacterMove` para
/// convertir el camino de `PathFinding` en direcciones. Coincide con `Direction` de OpenMU
/// menos 1 (`ToPacketByte`): 0 West, 1 SouthWest, 2 South, 3 SouthEast, 4 East, 5 NorthEast,
/// 6 North, 7 NorthWest.
pub const DIRECTIONS: [(i8, i8); 8] = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)];

/// Dirección (0..7) para ir de `from` a la casilla adyacente `to`; `None` si no son vecinas.
pub fn direction_between(from: (u8, u8), to: (u8, u8)) -> Option<u8> {
    let dx = to.0 as i16 - from.0 as i16;
    let dy = to.1 as i16 - from.1 as i16;
    DIRECTIONS.iter().position(|&(x, y)| x as i16 == dx && y as i16 == dy).map(|i| i as u8)
}

/// `KEY_GENERATE_FILTER` (KeyGenerater.cpp:9-21).
const KEY_GENERATE_FILTER: [[u32; 4]; 10] = [
    [321, 37531879, 8734, 32],
    [873, 64374332, 3546, 87],
    [537, 24798765, 5798, 32],
    [654, 32498765, 3573, 73],
    [546, 98465432, 6459, 12],
    [987, 24654876, 5616, 54],
    [357, 34599876, 8764, 98],
    [665, 78641332, 6547, 54],
    [813, 85132165, 8421, 98],
    [454, 57684216, 6875, 45],
];

/// `CKeyGenerater::GenerateKeyValue` (KeyGenerater.cpp:39-49): la clave siguiente de la de mover
/// de mapa. `DWORD` con desbordamiento; `f[2] / f[3]` es división entera.
pub fn map_move_key(key: u32) -> u32 {
    let f = KEY_GENERATE_FILTER[(key % 10) as usize];
    key.wrapping_mul(f[0]).wrapping_add(f[1]).wrapping_sub(f[2] / f[3])
}

// ---------------------------------------------------------------------------------------------
// Mensajes
// ---------------------------------------------------------------------------------------------

/// Lo que el cliente manda al Game Server (en claro; cifrar con `crypto::ClientCrypto`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMessage {
    Login { username: String, password: String, tick_count: u32, version: [u8; 5], serial: [u8; 16] },
    RequestCharacterList { language: u8 },
    SelectCharacter { name: String },
    /// `class` es el `CharacterClassNumber` (DW 0, DK 4, Elfa 8, MG 12, DL 16, Invocadora 20, RF 24).
    CreateCharacter { name: String, class: u8 },
    /// El cliente 1.04d manda siempre 20 bytes de código (máximo 20 aquí).
    DeleteCharacter { name: String, security_code: String },
    ClientReadyAfterMapChange,
    /// `directions` son direcciones 0..7 (ver [`DIRECTIONS`]), como mucho [`MAX_WALK_STEPS`]
    /// (las que sobren no se mandan). Sin pasos = "me he parado aquí mirando a `rotation`".
    Walk { source_x: u8, source_y: u8, rotation: u8, directions: Vec<u8> },
    Ping { tick_count: u32, attack_speed: u16, magic_speed: u16 },
    /// `kind`: ver [`logout_kind`].
    Logout { kind: u8 },
    /// `SendRequestMoveMap(dwBlockKey, wMapIndex)`: `key` es la siguiente de [`map_move_key`] e
    /// `index` el `MOVEREQINFO::index` elegido en la ventana de mover.
    WarpCommandRequest { key: u32, index: u16 },
}

/// Un personaje de la lista de la pantalla de selección.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterListEntry {
    pub slot: u8,
    pub name: String,
    pub level: u16,
    /// `CharacterStatus` (0 normal, 1 baneado). En la lista solo caben 4 bits: se manda
    /// `status & 0x0F` (el 32 de "GameMaster" no cabe; ese va en `CharacterInformation`).
    pub status: u8,
    /// `CharacterClassNumber`; en el cable va dentro de la apariencia (`clase << 3`).
    pub class: u8,
    /// `GuildMemberRole`: 0 miembro, 32 battle master, 64 asistente, 128 maestro, 255 sin gremio.
    pub guild_position: u8,
}

/// Datos del personaje al entrar al mundo (`CharacterInformation`, C3 F3 03, 72 bytes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CharacterInformation {
    pub x: u8,
    pub y: u8,
    pub map: u8,
    /// El cliente gira `(direction - 1) * 45` grados.
    pub direction: u8,
    pub experience: u64,
    pub next_experience: u64,
    pub level_up_points: u16,
    pub strength: u16,
    pub agility: u16,
    pub vitality: u16,
    pub energy: u16,
    pub health: u16,
    pub max_health: u16,
    pub mana: u16,
    pub max_mana: u16,
    pub shield: u16,
    pub max_shield: u16,
    pub ability: u16,
    pub max_ability: u16,
    pub money: u32,
    /// `CharacterHeroState`: 0 nuevo, 1 héroe, 2 héroe ligero, 3 normal, 4 aviso PK, 5/6 PK.
    pub hero_state: u8,
    /// `CharacterStatus`: 0 normal, 1 baneado, 32 GM.
    pub status: u8,
    pub used_fruit_points: u16,
    pub max_fruit_points: u16,
    pub leadership: u16,
    pub used_negative_fruit_points: u16,
    pub max_negative_fruit_points: u16,
    pub inventory_extensions: u8,
}

/// Un jugador que entra en el área de visión (`AddCharactersToScope`, variante S6 con efectos).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeCharacter {
    /// Id del objeto tal cual va en el cable: el servidor puede añadir [`NEW_OBJECT_FLAG`]
    /// si acaba de aparecer; el cliente debe quedarse con `id & OBJECT_ID_MASK`.
    pub id: u16,
    pub x: u8,
    pub y: u8,
    /// `CharacterClassNumber`; va en la apariencia (sin equipo).
    pub class: u8,
    pub name: String,
    pub target_x: u8,
    pub target_y: u8,
    /// Nibble alto del byte 34 (el cliente gira `(rotation - 1) * 45` grados).
    pub rotation: u8,
    /// `CharacterHeroState`, nibble bajo del byte 34.
    pub hero_state: u8,
}

/// Lo que el Game Server manda al cliente (ya descifrado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMessage {
    GameServerEntered { success: bool, player_id: u16, version: [u8; 5] },
    LoginResponse(LoginResult),
    /// Como mucho [`MAX_CHARACTER_LIST`] personajes (los que sobren no se mandan).
    /// `unlock_flags`: ver [`unlock`].
    CharacterList { unlock_flags: u8, move_count: u8, vault_extended: bool, characters: Vec<CharacterListEntry> },
    /// `flags`: ver [`unlock`].
    CharacterClassCreationUnlock { flags: u8 },
    CharacterCreated { name: String, slot: u8, level: u16, class: u8 },
    CharacterCreationFailed,
    /// `result`: ver [`delete_result`] (0 fallo, 1 borrado, 2 código incorrecto).
    CharacterDeleteResponse { result: u8 },
    CharacterInformation(CharacterInformation),
    /// Como mucho [`MAX_SCOPE_CHARACTERS`]; se mandan sin efectos.
    AddCharactersToScope(Vec<ScopeCharacter>),
    /// En S6 el cliente solo usa el destino y la rotación (recalcula el camino con
    /// `PathFinding2`); `directions` puede ir vacío. Si va, se empaqueta como en `Walk`.
    ObjectWalked { id: u16, target_x: u8, target_y: u8, rotation: u8, directions: Vec<u8> },
    /// Como mucho [`MAX_OUT_OF_SCOPE_IDS`] ids (los que sobren no se mandan).
    MapObjectOutOfScope(Vec<u16>),
    LogoutResponse { kind: u8 },
    /// La clave de mover de mapa (`PMSG_MAPMOVE_CHECKSUM`).
    MapMoveChecksumKey { key: u32 },
    /// Cambio de mapa o teletransporte (`PRECEIVE_TELEPORT_POSITION`). `map` es `WorldActive`
    /// (carpeta `World{map + 1}`); el cliente gira `(rotation - 1) * 45` grados.
    MapChanged { is_map_change: bool, map: u16, x: u8, y: u8, rotation: u8 },
    /// Zen del inventario (`ReceiveGetItem` con `Result` = `GET_ITEM_ZEN` = FE, WSclient.cpp:5607).
    InventoryMoneyUpdate { money: u32 },
}

// ---------------------------------------------------------------------------------------------
// Utilidades de (de)serialización
// ---------------------------------------------------------------------------------------------

fn fixed<const N: usize>(s: &str) -> [u8; N] {
    let mut out = [0u8; N];
    let n = s.len().min(N);
    out[..n].copy_from_slice(&s.as_bytes()[..n]);
    out
}

fn unfixed(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// Paquete C1 de tamaño fijo `len` con código y subcódigo; el resto a cero.
fn fixed_c1(len: usize, code: u8, sub_code: u8) -> Vec<u8> {
    debug_assert!(len <= 255);
    let mut p = vec![0u8; len];
    p[0] = 0xC1;
    p[1] = len as u8;
    p[2] = code;
    p[3] = sub_code;
    p
}

/// Igual que [`fixed_c1`] pero con cabecera C3 (la capa de cifrado lo cifra).
fn fixed_c3(len: usize, code: u8, sub_code: u8) -> Vec<u8> {
    let mut p = fixed_c1(len, code, sub_code);
    p[0] = 0xC3;
    p
}

fn put_u16_le(p: &mut [u8], i: usize, v: u16) {
    p[i..i + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32_le(p: &mut [u8], i: usize, v: u32) {
    p[i..i + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u64_be(p: &mut [u8], i: usize, v: u64) {
    p[i..i + 8].copy_from_slice(&v.to_be_bytes());
}

fn get_u16_le(p: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(p.get(i..i + 2)?.try_into().ok()?))
}

fn get_u16_be(p: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes(p.get(i..i + 2)?.try_into().ok()?))
}

fn get_u32_le(p: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?))
}

fn get_u64_be(p: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_be_bytes(p.get(i..i + 8)?.try_into().ok()?))
}

/// `<rotación:4 | pasos:4>` seguido de los pasos, dos por byte (el primero en el nibble alto),
/// como `SendCharacterMove` del cliente original y `DecodePayload` de OpenMU.
fn pack_steps(out: &mut Vec<u8>, rotation: u8, directions: &[u8]) {
    let directions = &directions[..directions.len().min(MAX_WALK_STEPS)];
    out.push((rotation << 4) | directions.len() as u8);
    for pair in directions.chunks(2) {
        let low = pair.get(1).map_or(0, |d| d & 0x0F);
        out.push(((pair[0] & 0x0F) << 4) | low);
    }
}

/// Inverso de [`pack_steps`]: `p[i]` es el byte de rotación/pasos.
fn unpack_steps(p: &[u8], i: usize) -> Option<(u8, Vec<u8>)> {
    let head = *p.get(i)?;
    let count = (head & 0x0F) as usize;
    let directions = (0..count)
        .map(|k| {
            let b = *p.get(i + 1 + k / 2)?;
            Some(if k % 2 == 0 { b >> 4 } else { b & 0x0F })
        })
        .collect::<Option<Vec<u8>>>()?;
    Some((head >> 4, directions))
}

/// Bytes del paquete si tiene cabecera de tamaño de 1 byte (C1/C3).
fn short_header<'a>(p: &Packet<'a>) -> Option<&'a [u8]> {
    (p.header.size_field_len() == 1).then_some(p.bytes)
}

/// Bytes del paquete si tiene cabecera de tamaño de 2 bytes (C2/C4).
fn long_header<'a>(p: &Packet<'a>) -> Option<&'a [u8]> {
    (p.header.size_field_len() == 2).then_some(p.bytes)
}

const CHARACTER_LIST_ENTRY_LEN: usize = 34;
const SCOPE_CHARACTER_LEN: usize = 36; // sin efectos
const CHARACTER_INFORMATION_LEN: usize = 72;
const CHARACTER_CREATED_LEN: usize = 42;
const MAP_CHANGED_LEN: usize = 15;

// ---------------------------------------------------------------------------------------------
// Cliente → servidor
// ---------------------------------------------------------------------------------------------

impl ClientMessage {
    pub fn login(username: &str, password: &str, tick_count: u32) -> Self {
        Self::Login {
            username: username.into(),
            password: password.into(),
            tick_count,
            version: CLIENT_VERSION,
            serial: CLIENT_SERIAL,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Login { username, password, tick_count, version, serial } => {
                let mut user = fixed::<10>(username);
                let mut pass = fixed::<20>(password);
                xor3(&mut user);
                xor3(&mut pass);
                let mut body = vec![0xF1, 0x01];
                body.extend_from_slice(&user);
                body.extend_from_slice(&pass);
                body.extend_from_slice(&tick_count.to_be_bytes());
                body.extend_from_slice(version);
                body.extend_from_slice(serial);
                body.push(0); // relleno: el paquete original mide 60 (el serial acaba en el 59)
                let mut p = c1(&body);
                p[0] = 0xC3; // se manda cifrado
                p
            }
            Self::RequestCharacterList { language } => c1(&[0xF3, 0x00, *language]),
            Self::SelectCharacter { name } => {
                let mut body = vec![0xF3, 0x03];
                body.extend_from_slice(&fixed::<10>(name));
                c1(&body)
            }
            Self::CreateCharacter { name, class } => {
                let mut p = fixed_c1(15, 0xF3, 0x01);
                p[4..14].copy_from_slice(&fixed::<10>(name));
                p[14] = (class << 2) & 0xFC;
                p
            }
            Self::DeleteCharacter { name, security_code } => {
                let mut p = fixed_c1(34, 0xF3, 0x02);
                p[4..14].copy_from_slice(&fixed::<10>(name));
                p[14..34].copy_from_slice(&fixed::<20>(security_code));
                p
            }
            Self::ClientReadyAfterMapChange => fixed_c1(4, 0xF3, 0x12),
            Self::Walk { source_x, source_y, rotation, directions } => {
                let mut body = vec![0xD4, *source_x, *source_y];
                pack_steps(&mut body, *rotation, directions);
                c1(&body)
            }
            Self::Ping { tick_count, attack_speed, magic_speed } => {
                let mut p = fixed_c3(12, 0x0E, 0x00);
                put_u32_le(&mut p, 4, *tick_count);
                put_u16_le(&mut p, 8, *attack_speed);
                put_u16_le(&mut p, 10, *magic_speed);
                p
            }
            Self::Logout { kind } => {
                let mut p = fixed_c3(5, 0xF1, 0x02);
                p[4] = *kind;
                p
            }
            Self::WarpCommandRequest { key, index } => {
                let mut p = fixed_c1(10, 0x8E, 0x02);
                put_u32_le(&mut p, 4, *key);
                put_u16_le(&mut p, 8, *index);
                p
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let p = Packet::parse(bytes)?;
        let d = short_header(&p)?;
        // Paquetes sin subcódigo.
        if p.code() == 0xD4 {
            let (rotation, directions) = unpack_steps(d, 5)?;
            return Some(Self::Walk { source_x: *d.get(3)?, source_y: *d.get(4)?, rotation, directions });
        }
        match (p.code(), p.sub_code()?) {
            (0xF1, 0x01) if d.len() >= 4 + 10 + 20 + 4 + 5 + 16 => {
                let mut user: [u8; 10] = d[4..14].try_into().ok()?;
                let mut pass: [u8; 20] = d[14..34].try_into().ok()?;
                xor3(&mut user);
                xor3(&mut pass);
                Some(Self::Login {
                    username: unfixed(&user),
                    password: unfixed(&pass),
                    tick_count: u32::from_be_bytes(d[34..38].try_into().ok()?),
                    version: d[38..43].try_into().ok()?,
                    serial: d[43..59].try_into().ok()?,
                })
            }
            (0xF1, 0x02) => Some(Self::Logout { kind: *d.get(4)? }),
            (0xF3, 0x00) => Some(Self::RequestCharacterList { language: *d.get(4).unwrap_or(&0) }),
            (0xF3, 0x01) => Some(Self::CreateCharacter { name: unfixed(d.get(4..14)?), class: *d.get(14)? >> 2 }),
            (0xF3, 0x02) => Some(Self::DeleteCharacter {
                name: unfixed(d.get(4..14)?),
                // Hay clientes que mandan menos de 20 bytes (solo los 7 del código).
                security_code: unfixed(d.get(14..).unwrap_or(&[])),
            }),
            (0xF3, 0x03) => Some(Self::SelectCharacter { name: unfixed(d.get(4..14)?) }),
            (0xF3, 0x12) => Some(Self::ClientReadyAfterMapChange),
            (0x0E, 0x00) => Some(Self::Ping {
                tick_count: get_u32_le(d, 4)?,
                attack_speed: get_u16_le(d, 8)?,
                magic_speed: get_u16_le(d, 10)?,
            }),
            (0x8E, 0x02) => Some(Self::WarpCommandRequest { key: get_u32_le(d, 4)?, index: get_u16_le(d, 8)? }),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Servidor → cliente
// ---------------------------------------------------------------------------------------------

impl CharacterInformation {
    fn encode(&self) -> Vec<u8> {
        let mut p = fixed_c3(CHARACTER_INFORMATION_LEN, 0xF3, 0x03);
        p[4] = self.x;
        p[5] = self.y;
        p[6] = self.map;
        p[7] = self.direction;
        put_u64_be(&mut p, 8, self.experience);
        put_u64_be(&mut p, 16, self.next_experience);
        put_u16_le(&mut p, 24, self.level_up_points);
        put_u16_le(&mut p, 26, self.strength);
        put_u16_le(&mut p, 28, self.agility);
        put_u16_le(&mut p, 30, self.vitality);
        put_u16_le(&mut p, 32, self.energy);
        put_u16_le(&mut p, 34, self.health);
        put_u16_le(&mut p, 36, self.max_health);
        put_u16_le(&mut p, 38, self.mana);
        put_u16_le(&mut p, 40, self.max_mana);
        put_u16_le(&mut p, 42, self.shield);
        put_u16_le(&mut p, 44, self.max_shield);
        put_u16_le(&mut p, 46, self.ability);
        put_u16_le(&mut p, 48, self.max_ability);
        // 50..52: relleno (el DWORD del zen va alineado a 4 en el struct del cliente).
        put_u32_le(&mut p, 52, self.money);
        p[56] = self.hero_state;
        p[57] = self.status;
        put_u16_le(&mut p, 58, self.used_fruit_points);
        put_u16_le(&mut p, 60, self.max_fruit_points);
        put_u16_le(&mut p, 62, self.leadership);
        put_u16_le(&mut p, 64, self.used_negative_fruit_points);
        put_u16_le(&mut p, 66, self.max_negative_fruit_points);
        p[68] = self.inventory_extensions;
        p
    }

    fn decode(d: &[u8]) -> Option<Self> {
        if d.len() < 69 {
            return None;
        }
        Some(Self {
            x: d[4],
            y: d[5],
            map: d[6],
            direction: d[7],
            experience: get_u64_be(d, 8)?,
            next_experience: get_u64_be(d, 16)?,
            level_up_points: get_u16_le(d, 24)?,
            strength: get_u16_le(d, 26)?,
            agility: get_u16_le(d, 28)?,
            vitality: get_u16_le(d, 30)?,
            energy: get_u16_le(d, 32)?,
            health: get_u16_le(d, 34)?,
            max_health: get_u16_le(d, 36)?,
            mana: get_u16_le(d, 38)?,
            max_mana: get_u16_le(d, 40)?,
            shield: get_u16_le(d, 42)?,
            max_shield: get_u16_le(d, 44)?,
            ability: get_u16_le(d, 46)?,
            max_ability: get_u16_le(d, 48)?,
            money: get_u32_le(d, 52)?,
            hero_state: d[56],
            status: d[57],
            used_fruit_points: get_u16_le(d, 58)?,
            max_fruit_points: get_u16_le(d, 60)?,
            leadership: get_u16_le(d, 62)?,
            used_negative_fruit_points: get_u16_le(d, 64)?,
            max_negative_fruit_points: get_u16_le(d, 66)?,
            inventory_extensions: d[68],
        })
    }
}

impl CharacterListEntry {
    fn write(&self, out: &mut Vec<u8>) {
        let start = out.len();
        out.resize(start + CHARACTER_LIST_ENTRY_LEN, 0);
        let e = &mut out[start..];
        e[0] = self.slot;
        e[1..11].copy_from_slice(&fixed::<10>(&self.name));
        // e[11]: relleno (el WORD del nivel va alineado a 2).
        put_u16_le(e, 12, self.level);
        e[14] = self.status & 0x0F; // nibble alto: IsItemBlockActive = false
        e[15..33].copy_from_slice(&appearance_for_class(self.class));
        e[33] = self.guild_position;
    }

    fn read(e: &[u8]) -> Option<Self> {
        let e = e.get(..CHARACTER_LIST_ENTRY_LEN)?;
        Some(Self {
            slot: e[0],
            name: unfixed(&e[1..11]),
            level: get_u16_le(e, 12)?,
            status: e[14] & 0x0F,
            class: class_from_appearance(&e[15..33]),
            guild_position: e[33],
        })
    }
}

impl ScopeCharacter {
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.id.to_be_bytes());
        out.push(self.x);
        out.push(self.y);
        out.extend_from_slice(&appearance_for_class(self.class));
        out.extend_from_slice(&fixed::<10>(&self.name));
        out.push(self.target_x);
        out.push(self.target_y);
        out.push((self.rotation << 4) | (self.hero_state & 0x0F));
        out.push(0); // sin efectos
    }

    /// Lee un personaje en `d[i..]`; devuelve también dónde empieza el siguiente.
    fn read(d: &[u8], i: usize) -> Option<(Self, usize)> {
        let e = d.get(i..i + SCOPE_CHARACTER_LEN)?;
        let effects = e[35] as usize;
        let next = i + SCOPE_CHARACTER_LEN + effects;
        if d.len() < next {
            return None;
        }
        let c = Self {
            id: get_u16_be(e, 0)?,
            x: e[2],
            y: e[3],
            class: class_from_appearance(&e[4..22]),
            name: unfixed(&e[22..32]),
            target_x: e[32],
            target_y: e[33],
            rotation: e[34] >> 4,
            hero_state: e[34] & 0x0F,
        };
        Some((c, next))
    }
}

impl ServerMessage {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::GameServerEntered { success, player_id, version } => {
                let mut body = vec![0xF1, 0x00, *success as u8];
                body.extend_from_slice(&player_id.to_be_bytes());
                body.extend_from_slice(version);
                c1(&body)
            }
            Self::LoginResponse(r) => c1(&[0xF1, 0x01, r.to_byte()]),
            Self::CharacterList { unlock_flags, move_count, vault_extended, characters } => {
                let characters = &characters[..characters.len().min(MAX_CHARACTER_LIST)];
                let mut body = vec![0xF3, 0x00, *unlock_flags, *move_count, characters.len() as u8, *vault_extended as u8];
                for c in characters {
                    c.write(&mut body);
                }
                c1(&body)
            }
            Self::CharacterClassCreationUnlock { flags } => c1(&[0xDE, 0x00, *flags]),
            Self::CharacterCreated { name, slot, level, class } => {
                let mut p = fixed_c1(CHARACTER_CREATED_LEN, 0xF3, 0x01);
                p[4] = 1;
                p[5..15].copy_from_slice(&fixed::<10>(name));
                p[15] = *slot;
                put_u16_le(&mut p, 16, *level);
                p[18] = class << 3;
                p[19] = 0; // CharacterStatus normal
                // PreviewData (20..42): OpenMU lo rellena entero de FF y el cliente lo ignora
                // (usa la clase); aquí va la apariencia sin equipo y el resto a FF.
                p[20..38].copy_from_slice(&appearance_for_class(*class));
                p[38..42].fill(0xFF);
                p
            }
            Self::CharacterCreationFailed => fixed_c1(5, 0xF3, 0x01),
            Self::CharacterDeleteResponse { result } => c1(&[0xF3, 0x02, *result]),
            Self::CharacterInformation(info) => info.encode(),
            Self::AddCharactersToScope(characters) => {
                let characters = &characters[..characters.len().min(MAX_SCOPE_CHARACTERS)];
                let mut body = Vec::with_capacity(2 + characters.len() * SCOPE_CHARACTER_LEN);
                body.push(0x12);
                body.push(characters.len() as u8);
                for c in characters {
                    c.write(&mut body);
                }
                c2(&body)
            }
            Self::ObjectWalked { id, target_x, target_y, rotation, directions } => {
                let mut body = vec![0xD4];
                body.extend_from_slice(&id.to_be_bytes());
                body.push(*target_x);
                body.push(*target_y);
                pack_steps(&mut body, *rotation, directions);
                c1(&body)
            }
            Self::MapObjectOutOfScope(ids) => {
                let ids = &ids[..ids.len().min(MAX_OUT_OF_SCOPE_IDS)];
                let mut body = vec![0x14, ids.len() as u8];
                for id in ids {
                    body.extend_from_slice(&id.to_be_bytes());
                }
                c1(&body)
            }
            Self::LogoutResponse { kind } => {
                let mut p = fixed_c3(5, 0xF1, 0x02);
                p[4] = *kind;
                p
            }
            Self::MapMoveChecksumKey { key } => {
                let mut p = fixed_c1(8, 0x8E, 0x01);
                put_u32_le(&mut p, 4, *key);
                p
            }
            Self::MapChanged { is_map_change, map, x, y, rotation } => {
                // `MapChanged` de OpenMU: 15 bytes, el 3 a 0F y del 10 al 14 a cero.
                let mut p = fixed_c3(MAP_CHANGED_LEN, 0x1C, 0x0F);
                p[4] = *is_map_change as u8;
                p[5..7].copy_from_slice(&map.to_be_bytes());
                p[7] = *x;
                p[8] = *y;
                p[9] = *rotation;
                p
            }
            Self::InventoryMoneyUpdate { money } => {
                let mut p = fixed_c3(8, 0x22, 0xFE);
                p[4..8].copy_from_slice(&money.to_be_bytes());
                p
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let p = Packet::parse(bytes)?;
        // Paquetes sin subcódigo.
        match p.code() {
            0x12 => {
                let d = long_header(&p)?;
                let count = *d.get(4)? as usize;
                let mut characters = Vec::with_capacity(count);
                let mut i = 5;
                for _ in 0..count {
                    let (c, next) = ScopeCharacter::read(d, i)?;
                    characters.push(c);
                    i = next;
                }
                return Some(Self::AddCharactersToScope(characters));
            }
            0x14 => {
                let d = short_header(&p)?;
                let count = *d.get(3)? as usize;
                let ids = (0..count).map(|k| get_u16_be(d, 4 + 2 * k)).collect::<Option<Vec<u16>>>()?;
                return Some(Self::MapObjectOutOfScope(ids));
            }
            0xD4 => {
                let d = short_header(&p)?;
                let (rotation, directions) = unpack_steps(d, 7)?;
                return Some(Self::ObjectWalked {
                    id: get_u16_be(d, 3)?,
                    target_x: *d.get(5)?,
                    target_y: *d.get(6)?,
                    rotation,
                    directions,
                });
            }
            _ => {}
        }
        let d = short_header(&p)?;
        match (p.code(), p.sub_code()?) {
            (0xF1, 0x00) => Some(Self::GameServerEntered {
                success: *d.get(4)? != 0,
                player_id: get_u16_be(d, 5)?,
                version: d.get(7..12)?.try_into().ok()?,
            }),
            (0xF1, 0x01) => Some(Self::LoginResponse(LoginResult::from_byte(*d.get(4)?))),
            (0xF1, 0x02) => Some(Self::LogoutResponse { kind: *d.get(4)? }),
            (0xF3, 0x00) => {
                let count = *d.get(6)? as usize;
                let characters = (0..count)
                    .map(|k| CharacterListEntry::read(d.get(8 + k * CHARACTER_LIST_ENTRY_LEN..)?))
                    .collect::<Option<Vec<_>>>()?;
                Some(Self::CharacterList {
                    unlock_flags: d[4],
                    move_count: d[5],
                    vault_extended: *d.get(7)? != 0,
                    characters,
                })
            }
            (0xF3, 0x01) => match *d.get(4)? {
                1 => Some(Self::CharacterCreated {
                    name: unfixed(d.get(5..15)?),
                    slot: *d.get(15)?,
                    level: get_u16_le(d, 16)?,
                    class: *d.get(18)? >> 3,
                }),
                // 0 = fallo; el cliente original también entiende 2 como otro fallo.
                _ => Some(Self::CharacterCreationFailed),
            },
            (0xF3, 0x02) => Some(Self::CharacterDeleteResponse { result: *d.get(4)? }),
            (0xF3, 0x03) => Some(Self::CharacterInformation(CharacterInformation::decode(d)?)),
            (0xDE, 0x00) => Some(Self::CharacterClassCreationUnlock { flags: *d.get(4)? }),
            (0x8E, 0x01) => Some(Self::MapMoveChecksumKey { key: get_u32_le(d, 4)? }),
            // `ReceiveTeleport` no mira el byte 3 (ni el largo: `MapChangedExtended` mide 16).
            (0x1C, _) if d.len() >= 10 => Some(Self::MapChanged {
                // `WORD Flag` (bytes 4 y 5) del original; con los mapas del S6 (< 256) el byte 5
                // (alto del mapa) es 0 y basta el 4, como `IsMapChange` de OpenMU.
                is_map_change: d[4] != 0,
                map: get_u16_be(d, 5)?,
                x: d[7],
                y: d[8],
                rotation: d[9],
            }),
            // `Item[0..4]` en big endian.
            (0x22, 0xFE) => Some(Self::InventoryMoneyUpdate { money: u32::from_be_bytes(d.get(4..8)?.try_into().ok()?) }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{ClientCrypto, ServerCrypto};

    fn ida_y_vuelta_cliente(msg: ClientMessage) -> Vec<u8> {
        let wire = msg.encode();
        assert_eq!(ClientMessage::decode(&wire), Some(msg.clone()), "ida y vuelta de {msg:?}");
        wire
    }

    fn ida_y_vuelta_servidor(msg: ServerMessage) -> Vec<u8> {
        let wire = msg.encode();
        assert_eq!(ServerMessage::decode(&wire), Some(msg.clone()), "ida y vuelta de {msg:?}");
        wire
    }

    fn info_de_prueba() -> CharacterInformation {
        CharacterInformation {
            x: 130,
            y: 125,
            map: 0,
            direction: 3,
            experience: 0x0102_0304_0506_0708,
            next_experience: 100_000,
            level_up_points: 5,
            strength: 28,
            agility: 20,
            vitality: 25,
            energy: 10,
            health: 110,
            max_health: 110,
            mana: 20,
            max_mana: 20,
            shield: 60,
            max_shield: 60,
            ability: 12,
            max_ability: 12,
            money: 1_000_000,
            hero_state: 3,
            status: 32,
            used_fruit_points: 1,
            max_fruit_points: 2,
            leadership: 0,
            used_negative_fruit_points: 3,
            max_negative_fruit_points: 4,
            inventory_extensions: 2,
        }
    }

    #[test]
    fn tamanos_como_en_openmu() {
        let login = ClientMessage::login("test", "secreto", 12345).encode();
        assert_eq!(login.len(), 60, "LoginLongPassword mide 60");
        assert_eq!(&login[..4], &[0xC3, 60, 0xF1, 0x01]);
        // Usuario en el índice 4, contraseña en el 14, tick en el 34, versión en el 38, serial en el 43.
        assert_eq!(&login[38..43], b"10404");
        assert_eq!(&login[43..59], b"k1Pk2jcET48mxL3b");
        assert_eq!(u32::from_be_bytes(login[34..38].try_into().unwrap()), 12345);
        assert_ne!(&login[4..8], b"test", "el usuario va con Xor3");

        assert_eq!(ClientMessage::RequestCharacterList { language: 0 }.encode().len(), 5);
        assert_eq!(ClientMessage::SelectCharacter { name: "Pepe".into() }.encode().len(), 14);
        let entered = ServerMessage::GameServerEntered { success: true, player_id: 0x1234, version: CLIENT_VERSION };
        assert_eq!(entered.encode().len(), 12);
    }

    #[test]
    fn login_cifrado_de_punta_a_punta() {
        let mut client = ClientCrypto::default();
        let mut server = ServerCrypto::default();

        let msg = ClientMessage::login("jugador", "contraseña", 987654);
        let wire = client.encrypt(&msg.encode());
        assert_eq!(wire[0], 0xC3);
        let plain = server.decrypt(&wire).unwrap();
        assert_eq!(ClientMessage::decode(&plain), Some(msg));

        let reply = ServerMessage::LoginResponse(LoginResult::Okay);
        assert_eq!(ServerMessage::decode(&client.decrypt(&server.encrypt(&reply.encode())).unwrap()), Some(reply));

        let list = ClientMessage::RequestCharacterList { language: 0 };
        assert_eq!(ClientMessage::decode(&server.decrypt(&client.encrypt(&list.encode())).unwrap()), Some(list));
    }

    #[test]
    fn mensajes_existentes_ida_y_vuelta() {
        ida_y_vuelta_cliente(ClientMessage::login("a", "b", 1));
        ida_y_vuelta_cliente(ClientMessage::RequestCharacterList { language: 2 });
        ida_y_vuelta_cliente(ClientMessage::SelectCharacter { name: "Pepe".into() });
        ida_y_vuelta_servidor(ServerMessage::GameServerEntered { success: true, player_id: 0x1234, version: CLIENT_VERSION });
        ida_y_vuelta_servidor(ServerMessage::LoginResponse(LoginResult::AccountBlocked));
    }

    #[test]
    fn clases() {
        let esperado = [(0, 0), (1, 4), (2, 8), (3, 12), (4, 16), (5, 20), (6, 24)];
        for (indice, numero) in esperado {
            assert_eq!(class_number(indice), numero);
            assert_eq!(base_class_index(numero), indice);
        }
        assert_eq!(base_class_index(7), 1, "Blade Master sigue siendo DK");
        assert_eq!(base_class_index(25), 6, "Fist Master sigue siendo RF");
    }

    #[test]
    fn apariencia_sin_equipo() {
        let dk = appearance_for_class(4);
        assert_eq!(
            dk,
            [0x20, 0xFF, 0xFF, 0xFF, 0xFF, 0xF3, 0, 0, 0, 0xF8, 0, 0, 0xF0, 0xFF, 0xFF, 0xFF, 0, 0]
        );
        for class in [0, 3, 4, 7, 8, 12, 16, 20, 23, 24, 25] {
            assert_eq!(class_from_appearance(&appearance_for_class(class)), class);
        }
    }

    #[test]
    fn direcciones() {
        for (i, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
            let to = ((100 + dx as i16) as u8, (100 + dy as i16) as u8);
            assert_eq!(direction_between((100, 100), to), Some(i as u8));
        }
        assert_eq!(direction_between((10, 10), (10, 10)), None);
        assert_eq!(direction_between((10, 10), (12, 10)), None);
        assert_eq!(direction_between((0, 0), (255, 255)), None, "sin dar la vuelta");
        // Tabla del cliente original: 0 = (-1,-1), 3 = (+1, 0), 7 = (-1, 0).
        assert_eq!(direction_between((5, 5), (4, 4)), Some(0));
        assert_eq!(direction_between((5, 5), (6, 5)), Some(3));
        assert_eq!(direction_between((5, 5), (4, 5)), Some(7));
    }

    #[test]
    fn crear_y_borrar_personaje() {
        let crear = ida_y_vuelta_cliente(ClientMessage::CreateCharacter { name: "Guerrero".into(), class: 4 });
        assert_eq!(crear.len(), 15);
        assert_eq!(&crear[..4], &[0xC1, 15, 0xF3, 0x01]);
        assert_eq!(crear[14], 0x10, "DK = índice 1 << 4 como manda el cliente original");
        ida_y_vuelta_cliente(ClientMessage::CreateCharacter { name: "Luchador".into(), class: 24 });

        let borrar = ida_y_vuelta_cliente(ClientMessage::DeleteCharacter { name: "Guerrero".into(), security_code: "1234567".into() });
        assert_eq!(borrar.len(), 34);

        let creado = ida_y_vuelta_servidor(ServerMessage::CharacterCreated { name: "Guerrero".into(), slot: 2, level: 1, class: 4 });
        assert_eq!(creado.len(), 42);
        assert_eq!(creado[4], 1);
        assert_eq!(creado[18], 0x20, "DK << 3");
        assert_eq!(&creado[20..38], &appearance_for_class(4));

        let fallo = ida_y_vuelta_servidor(ServerMessage::CharacterCreationFailed);
        assert_eq!(fallo, [0xC1, 5, 0xF3, 0x01, 0]);

        for result in [delete_result::UNSUCCESSFUL, delete_result::SUCCESSFUL, delete_result::WRONG_SECURITY_CODE] {
            let r = ida_y_vuelta_servidor(ServerMessage::CharacterDeleteResponse { result });
            assert_eq!(r.len(), 5);
        }

        let unlock = ida_y_vuelta_servidor(ServerMessage::CharacterClassCreationUnlock {
            flags: unlock::SUMMONER | unlock::RAGE_FIGHTER,
        });
        assert_eq!(unlock, [0xC1, 5, 0xDE, 0x00, 9]);
    }

    #[test]
    fn lista_de_personajes() {
        let characters = vec![
            CharacterListEntry { slot: 0, name: "Mago".into(), level: 400, status: 0, class: 3, guild_position: 255 },
            CharacterListEntry { slot: 1, name: "Elfa".into(), level: 1, status: 1, class: 8, guild_position: 128 },
            CharacterListEntry { slot: 4, name: "Lord".into(), level: 250, status: 0, class: 17, guild_position: 0 },
        ];
        let msg = ServerMessage::CharacterList { unlock_flags: 0x0F, move_count: 3, vault_extended: true, characters };
        let wire = ida_y_vuelta_servidor(msg);
        assert_eq!(wire.len(), 8 + 3 * 34);
        assert_eq!(&wire[..8], &[0xC1, (8 + 3 * 34) as u8, 0xF3, 0x00, 0x0F, 3, 3, 1]);
        // Primer personaje: nivel LE en 8+12, apariencia en 8+15 (Grand Master = 3 << 3).
        assert_eq!(&wire[20..22], &400u16.to_le_bytes());
        assert_eq!(wire[23], 3 << 3);
        assert_eq!(wire[8 + 33], 255);

        let vacia = ida_y_vuelta_servidor(ServerMessage::CharacterList {
            unlock_flags: 0,
            move_count: 0,
            vault_extended: false,
            characters: vec![],
        });
        assert_eq!(vacia.len(), 8);
    }

    #[test]
    fn informacion_del_personaje() {
        let wire = ida_y_vuelta_servidor(ServerMessage::CharacterInformation(info_de_prueba()));
        assert_eq!(wire.len(), 72);
        assert_eq!(&wire[..4], &[0xC3, 72, 0xF3, 0x03]);
        assert_eq!(&wire[8..16], &[1, 2, 3, 4, 5, 6, 7, 8], "experiencia en big endian");
        assert_eq!(&wire[52..56], &1_000_000u32.to_le_bytes());
        ida_y_vuelta_servidor(ServerMessage::CharacterInformation(CharacterInformation::default()));
    }

    #[test]
    fn mapa_ping_y_salida() {
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::ClientReadyAfterMapChange), [0xC1, 4, 0xF3, 0x12]);

        let ping = ida_y_vuelta_cliente(ClientMessage::Ping { tick_count: 0xAABBCCDD, attack_speed: 300, magic_speed: 150 });
        assert_eq!(ping.len(), 12);
        assert_eq!(&ping[..4], &[0xC3, 12, 0x0E, 0x00]);
        assert_eq!(&ping[4..8], &[0xDD, 0xCC, 0xBB, 0xAA]);

        for kind in [logout_kind::CLOSE_GAME, logout_kind::BACK_TO_CHARACTER_SELECTION, logout_kind::BACK_TO_SERVER_SELECTION] {
            assert_eq!(ida_y_vuelta_cliente(ClientMessage::Logout { kind }), [0xC3, 5, 0xF1, 0x02, kind]);
            assert_eq!(ida_y_vuelta_servidor(ServerMessage::LogoutResponse { kind }), [0xC3, 5, 0xF1, 0x02, kind]);
        }
    }

    #[test]
    fn caminar() {
        let walk = ida_y_vuelta_cliente(ClientMessage::Walk { source_x: 130, source_y: 125, rotation: 5, directions: vec![3, 4, 5] });
        // Rotación en el nibble alto, pasos en el bajo; luego dos pasos por byte, el primero arriba.
        assert_eq!(walk, [0xC1, 8, 0xD4, 130, 125, 0x53, 0x34, 0x50]);

        let parado = ida_y_vuelta_cliente(ClientMessage::Walk { source_x: 10, source_y: 20, rotation: 2, directions: vec![] });
        assert_eq!(parado, [0xC1, 6, 0xD4, 10, 20, 0x20]);

        let largo = ClientMessage::Walk { source_x: 1, source_y: 1, rotation: 0, directions: vec![1; 20] };
        match ClientMessage::decode(&largo.encode()) {
            Some(ClientMessage::Walk { directions, .. }) => assert_eq!(directions.len(), MAX_WALK_STEPS),
            otro => panic!("{otro:?}"),
        }

        // El cliente original manda siempre 13 bytes (Path[8]) aunque haya menos pasos.
        let original = [0xC1, 13, 0xD4, 50, 60, 0x12, 0x70, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            ClientMessage::decode(&original),
            Some(ClientMessage::Walk { source_x: 50, source_y: 60, rotation: 1, directions: vec![7, 0] })
        );

        let visto = ida_y_vuelta_servidor(ServerMessage::ObjectWalked {
            id: 0x1234,
            target_x: 140,
            target_y: 130,
            rotation: 4,
            directions: vec![],
        });
        assert_eq!(visto, [0xC1, 8, 0xD4, 0x12, 0x34, 140, 130, 0x40]);
        let con_pasos = ida_y_vuelta_servidor(ServerMessage::ObjectWalked {
            id: 7,
            target_x: 1,
            target_y: 2,
            rotation: 6,
            directions: vec![0, 1, 2, 3, 4],
        });
        assert_eq!(con_pasos.len(), 8 + 3);
    }

    #[test]
    fn jugadores_que_entran_y_salen_de_vista() {
        let jugadores = vec![
            ScopeCharacter {
                id: 0x8000 | 123,
                x: 130,
                y: 125,
                class: 4,
                name: "Guerrero".into(),
                target_x: 132,
                target_y: 127,
                rotation: 3,
                hero_state: 3,
            },
            ScopeCharacter { id: 456, x: 1, y: 2, class: 20, name: "Invocadora".into(), target_x: 1, target_y: 2, rotation: 0, hero_state: 0 },
        ];
        let wire = ida_y_vuelta_servidor(ServerMessage::AddCharactersToScope(jugadores));
        assert_eq!(wire.len(), 5 + 2 * 36);
        assert_eq!(&wire[..5], &[0xC2, 0, (5 + 2 * 36) as u8, 0x12, 2]);
        assert_eq!(&wire[5..7], &[0x80, 123], "el id viaja tal cual, con su bit alto");
        assert_eq!(&wire[9..27], &appearance_for_class(4));
        assert_eq!(wire[5 + 34], 0x33);
        assert_eq!(wire[5 + 35], 0, "sin efectos");
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::AddCharactersToScope(vec![])).len(), 5);

        // Un personaje con efectos (como los manda OpenMU) también se lee.
        let mut con_efectos = wire[3..5 + 36].to_vec();
        con_efectos[1] = 1;
        con_efectos[2 + 35] = 2;
        con_efectos.extend_from_slice(&[10, 20]);
        let paquete = c2(&con_efectos);
        match ServerMessage::decode(&paquete) {
            Some(ServerMessage::AddCharactersToScope(v)) => assert_eq!(v[0].name, "Guerrero"),
            otro => panic!("{otro:?}"),
        }

        let fuera = ida_y_vuelta_servidor(ServerMessage::MapObjectOutOfScope(vec![1, 0x1234, 0x7FFF]));
        assert_eq!(fuera, [0xC1, 10, 0x14, 3, 0, 1, 0x12, 0x34, 0x7F, 0xFF]);
        let muchos = ServerMessage::MapObjectOutOfScope((0..300).collect()).encode();
        assert_eq!(muchos.len(), 4 + 2 * MAX_OUT_OF_SCOPE_IDS);
    }

    #[test]
    fn clave_de_mover() {
        // A mano con `GenerateKeyValue`: 0 % 10 = 0 → 0 * 321 + 37531879 - 8734 / 32 (= 272).
        assert_eq!(map_move_key(0), 37_531_879 - 272);
        // 1234567 % 10 = 7 → 1234567 * 665 + 78641332 - 6547 / 54 (= 121).
        assert_eq!(map_move_key(1_234_567), 1_234_567u32.wrapping_mul(665) + 78_641_332 - 121);
        // DWORD: desborda sin fallar (u32::MAX % 10 = 5 → -987 + 24654876 - 5616 / 54).
        assert_eq!(map_move_key(u32::MAX), 24_654_876 - 987 - 104);
    }

    #[test]
    fn mover_de_mapa() {
        let pedir = ida_y_vuelta_cliente(ClientMessage::WarpCommandRequest { key: 0x1122_3344, index: 23 });
        // `spe << (BYTE)0x02 << (DWORD)dwBlockKey << (WORD)wMapIndex`.
        assert_eq!(pedir, [0xC1, 10, 0x8E, 0x02, 0x44, 0x33, 0x22, 0x11, 23, 0]);

        let clave = ida_y_vuelta_servidor(ServerMessage::MapMoveChecksumKey { key: 0xA1B2_C3D4 });
        assert_eq!(clave, [0xC1, 8, 0x8E, 0x01, 0xD4, 0xC3, 0xB2, 0xA1]);

        let cambio = ida_y_vuelta_servidor(ServerMessage::MapChanged { is_map_change: true, map: 3, x: 175, y: 112, rotation: 1 });
        assert_eq!(cambio, [0xC3, 15, 0x1C, 0x0F, 1, 0, 3, 175, 112, 1, 0, 0, 0, 0, 0]);
        ida_y_vuelta_servidor(ServerMessage::MapChanged { is_map_change: false, map: 0, x: 1, y: 2, rotation: 0 });
        // `MapChangedExtended` (16, con el discriminador) también se lee.
        let mut extendido = cambio.clone();
        extendido[1] = 16;
        extendido.push(7);
        assert_eq!(
            ServerMessage::decode(&extendido),
            Some(ServerMessage::MapChanged { is_map_change: true, map: 3, x: 175, y: 112, rotation: 1 })
        );

        let zen = ida_y_vuelta_servidor(ServerMessage::InventoryMoneyUpdate { money: 0x0001_E240 });
        assert_eq!(zen, [0xC3, 8, 0x22, 0xFE, 0x00, 0x01, 0xE2, 0x40], "zen en big endian");
        // Cifrados de punta a punta.
        let mut client = ClientCrypto::default();
        let mut server = ServerCrypto::default();
        for m in [ServerMessage::MapChanged { is_map_change: true, map: 51, x: 52, y: 225, rotation: 0 }, ServerMessage::InventoryMoneyUpdate { money: 5 }] {
            assert_eq!(ServerMessage::decode(&client.decrypt(&server.encrypt(&m.encode())).unwrap()), Some(m));
        }
        let w = ClientMessage::WarpCommandRequest { key: 9, index: 42 };
        assert_eq!(ClientMessage::decode(&server.decrypt(&client.encrypt(&w.encode())).unwrap()), Some(w));
    }

    #[test]
    fn c3_cifrados_de_punta_a_punta() {
        let mut client = ClientCrypto::default();
        let mut server = ServerCrypto::default();

        let info = ServerMessage::CharacterInformation(info_de_prueba());
        let wire = server.encrypt(&info.encode());
        assert_eq!(wire[0], 0xC3);
        assert_eq!(ServerMessage::decode(&client.decrypt(&wire).unwrap()), Some(info));

        let ping = ClientMessage::Ping { tick_count: 42, attack_speed: 10, magic_speed: 20 };
        let wire = client.encrypt(&ping.encode());
        assert_eq!(wire[0], 0xC3);
        assert_eq!(ClientMessage::decode(&server.decrypt(&wire).unwrap()), Some(ping));

        let logout = ClientMessage::Logout { kind: logout_kind::BACK_TO_CHARACTER_SELECTION };
        assert_eq!(ClientMessage::decode(&server.decrypt(&client.encrypt(&logout.encode())).unwrap()), Some(logout));
        let resp = ServerMessage::LogoutResponse { kind: 1 };
        assert_eq!(ServerMessage::decode(&client.decrypt(&server.encrypt(&resp.encode())).unwrap()), Some(resp));

        // Un C1 en claro (caminar) también pasa por la capa de cifrado del cliente (Xor32).
        let walk = ClientMessage::Walk { source_x: 1, source_y: 2, rotation: 3, directions: vec![4, 5] };
        assert_eq!(ClientMessage::decode(&server.decrypt(&client.encrypt(&walk.encode())).unwrap()), Some(walk));
    }
}
