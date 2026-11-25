//! Game Server packets: entry, login, character selection and the basic world
//! (appearing on the map, walking, seeing other players and leaving).
//!
//! Indices are absolute (they count the header), same as in OpenMU's XMLs.
//!
//! ```text
//! server → client  GameServerEntered            C1 0C F1 00 <ok u8> <player id u16 BE> <version 5>
//! client → server  Login                        C3 3C F1 01 <user 10, Xor3> <password 20, Xor3>
//!                                                           <tick u32 BE> <version 5> <serial 16>
//! server → client  LoginResponse                C1 05 F1 01 <result>
//! client → server  Logout                       C3 05 F1 02 <kind: 0 close, 1 pick character, 2 pick server>
//! server → client  LogoutResponse               C3 05 F1 02 <kind>
//! client → server  RequestCharacterList         C1 05 F3 00 <language>
//! server → client  CharacterList                C1 len F3 00 <unlocks> <moves> <n> <vault ext.>
//!                                                           n × [<slot> <name 10> 00 <level u16 LE>
//!                                                                <status:4 | lock:4> <appearance 18> <guild rank>]
//! server → client  CharacterClassCreationUnlock C1 05 DE 00 <unlocks>
//! client → server  CreateCharacter              C1 0F F3 01 <name 10> <class << 2>
//! server → client  CharacterCreationSuccessful  C1 2A F3 01 01 <name 10> <slot> <level u16 LE> <class << 3>
//!                                                           <status> <appearance 18> FF FF FF FF
//! server → client  CharacterCreationFailed      C1 05 F3 01 00
//! client → server  DeleteCharacter              C1 22 F3 02 <name 10> <security code 20>
//! server → client  CharacterDeleteResponse      C1 05 F3 02 <result>
//! client → server  SelectCharacter              C1 0E F3 03 <name 10>
//! server → client  CharacterInformation         C3 48 F3 03 <x> <y> <map> <direction> <exp u64 BE> <next exp u64 BE>
//!                                                           <points u16 LE> <strength> <agility> <vitality> <energy>
//!                                                           <life> <max life> <mana> <max mana> <shield> <max shield>
//!                                                           <AG> <max AG> 00 00 <zen u32 LE> <hero> <status>
//!                                                           <fruits> <max fruits> <command> <fruits -> <max fruits ->
//!                                                           <inventory extensions> 00 00 00   (all u16 LE)
//! client → server  ClientReadyAfterMapChange    C1 04 F3 12
//! client → server  Ping                         C3 0C 0E 00 <tick u32 LE> <attack speed u16 LE> <magic speed u16 LE>
//! client → server  Walk (WalkRequest)           C1 len D4 <source x> <source y> <rotation:4 | steps:4> <steps, 2 per byte>
//! server → client  ObjectWalked                 C1 len D4 <id u16 BE> <target x> <target y> <rotation:4 | steps:4>
//!                                                           <steps, 2 per byte>
//! server → client  AddCharactersToScope         C2 len 12 <n> n × [<id u16 BE> <x> <y> <appearance 18> <name 10>
//!                                                                 <target x> <target y> <rotation:4 | hero:4>
//!                                                                 <n effects> <effects n>]
//!                                               (appearance: see [`crate::appearance`])
//! server → client  AppearanceChanged            C1 11 25 <id u16 BE> <item 12, Item[1] = slot << 4 | glow level>
//! server → client  MapObjectOutOfScope          C1 len 14 <n> n × <id u16 BE>
//! server → client  MapMoveChecksumKey           C1 08 8E 01 <key u32 LE>
//! client → server  WarpCommandRequest           C1 0A 8E 02 <key u32 LE> <index u16 LE>
//! server → client  MapChanged                   C3 0F 1C 0F <map change> <map u16 BE> <x> <y>
//!                                                           <rotation> 00 00 00 00 00
//! server → client  InventoryMoneyUpdate         C3 08 22 FE <zen u32 BE>
//! client → server  VaultMoveMoneyRequest        C1 08 81 <0 deposit, 1 withdraw> <zen u32 LE>
//! server → client  VaultMoneyUpdate             C1 0C 81 <ok> <vault zen u32 LE> <inventory zen u32 LE>
//! client → server  VaultClosed                  C1 03 82
//! server → client  VaultClosed                  C1 03 82
//! client → server  UnlockVault / SetVaultPin /  C3 1A 83 <0, 1, 2> <pin u16 LE> <password 20>
//!                  RemoveVaultPin
//! server → client  VaultProtectionInformation   C1 04 83 <state>
//! client → server  PublicChatMessage            C1 len 00 <name 10> <text, UTF-8 with its 00, at most 90>
//! client → server  WhisperMessage               C1 len 02 <recipient 10> <text, UTF-8 with its 00, at most 90>
//! server → client  ChatMessage                  C1 len 00 <speaker 10> <text with its 00>
//! server → client  WhisperMessage               C1 len 02 <whisperer 10> <text with its 00>
//! server → client  WhisperResult                C1 04 0C <0 = not connected>
//! server → client  ServerMessage (notice)       C1 len 0D <kind> <count> 00 <delay u16 LE> <color u32 LE>
//!                                                           <speed> <text with its 00>
//! client → server  EnterGateRequest             C3 08 1C 00 <gate u16 LE> <x> <y>
//! client → server  AnimationRequest             C1 05 18 <direction> <action AT_*>
//! server → client  ObjectAnimation              C1 09 18 <id u16 BE> <direction> <action AT_*> <target id u16 BE>
//! ```
//!
//! # Walk-in gates
//!
//! `CheckGate` (ZzzInterface.cpp:7023-7149) checks every tick whether the hero steps on an entry gate
//! (`Flag` 1) of its map and, if the level is enough, sends `SendRequestMagicTeleport(&bResult, gate,
//! 0, 0)` (wsclientinline.h:655-675): `spe.Init(0xC1, 0x1C)`, a null byte, `WORD` gate and the
//! two tiles (only used by the wizard's teleport, with gate 0), with `Send(TRUE)` (encrypted:
//! C3, like OpenMU's `EnterGateRequest`). The server replies with `MapChanged` like the `M` window.
//!
//! # Animations (`AT_*`)
//!
//! `SendRequestAction(action, direction)` (wsclientinline.h: `spe.Init(0xC1, 0x18)`, `BYTE` direction
//! and `BYTE` action) when sitting, leaning or healing (ZzzInterface.cpp:3700-3733) and with the chat
//! gestures (`CheckChatText`, ZzzInterface.cpp:4434-4590). The direction is
//! `((Angle + 22.5) / 360 * 8 + 1) % 8`, the same as in `MapChanged` (the receiver turns
//! `(direction − 1) * 45`). The server relays it to those who see it (OpenMU's `AnimationHandlerPlugIn` →
//! `ObjectAnimation`), read by `ReceiveAction` (`PRECEIVE_ACTION`, WSclient.h:728-736 and
//! WSclient.cpp:3120-3353). See [`action`].
//!
//! # Chat
//!
//! `SendChat` and `SendChatWhisper` (wsclientinline.h:328-404) send `PCHATING` (WSclient.h:503-507):
//! the name in 10 bytes and the text with its zero, `min(strlen + 1, MAX_CHAT_SIZE)` bytes
//! (`MAX_CHAT_SIZE` = 90). The client converts it to UTF-8 before sending (NewUIChatInputBox.cpp:537).
//! The server replies with the same structure and code: `ReceiveChat` (0x00) and
//! `ReceiveChatWhisper` (0x02), WSclient.cpp:1417-1539. The whisper result (0x0C,
//! `ReceiveChatWhisperResult`, WSclient.cpp:1541) is only checked for 0: "not there".
//!
//! The server notice is `PRECEIVE_NOTICE` (WSclient.h:516-524), without `pack`: `Result` at 3,
//! `Count` at 4, `Delay` (WORD) at 6, `Color` (DWORD) at 8, `Speed` at 12 and the text at
//! 13 for all kinds (`ReceiveNotice`, WSclient.cpp:1577). OpenMU sends it the same way for S6:
//! `ServerMessage` with the text at 4 but preceded by nine "0" (ShowMessagePlugIn.cs).
//!
//! # Map move (`M`)
//!
//! On entering, the server sends the key (`ReceiveMoveMapChecksum`, WSclient.cpp:11932). Each
//! `SendRequestMoveMap` (wsclientinline.h:1217) carries the next one: `GetMoveCommandKey` runs it through
//! [`map_move_key`] and keeps it (NewUIMoveCommandWindow.cpp:273-278); the server does the same
//! with its own (`CheckKeyValue`). If valid, it takes the zen (`InventoryMoneyUpdate`), removes the
//! character from the view of those on the map and sends `MapChanged`; the client loads the map and replies
//! `ClientReadyAfterMapChange` (`SendRequestFinishLoading`, WSclient.cpp:1814), and then the
//! server puts it in the view of those on the new map (like OpenMU's `PlayerMapTransitions`).
//!
//! `MapChanged` is read by `ReceiveTeleport` (WSclient.cpp:1698) with `PRECEIVE_TELEPORT_POSITION`
//! (WSclient.h, without `pack`): `WORD Flag` at byte 4 (5 is OpenMU's map high byte),
//! `Map` at 6, `PositionX/Y` at 7 and 8 and `Angle` at 9. `Flag` 0 is a teleport within the
//! same map (without clearing the others); any other value, a map change. OpenMU's (`MapChanged`) is 15 long.
//!
//! Formats taken from `OpenMU/src/Network/Packets/*.xml` (MIT license), Season 6 Ep3 variant,
//! checked against the original client (`Main5.2`: `WSclient.cpp`, `wsclientinline.h`, `ProtocolSend.cpp`).
//!
//! # Classes on the wire
//!
//! Classes are stored as OpenMU's `CharacterClassNumber` (`base class * 4 + evolution`:
//! DW 0, SM 2, GM 3, DK 4, BK 6, BM 7, Elf 8, ME 10, HE 11, MG 12, DM 13, DL 16, LE 17,
//! Summoner 20, BS 22, DiM 23, RF 24, FM 25). Depending on the packet they are shifted:
//! - `CreateCharacter`: `class << 2` (the client sends `(index << 4) + skin`).
//! - `CharacterCreationSuccessful` and byte 0 of the appearance: `class << 3`
//!   (the client undoes it in `ChangeServerClassTypeToClientClassType`).

use crate::appearance::{self, Appearance, VisibleItem};
use crate::packet::{Packet, c1, c2};
use crate::xor::xor3;

/// Version and serial of the Season 6 Ep3 client OpenMU accepts by default ("1.04d", GMO).
pub const CLIENT_VERSION: [u8; 5] = *b"10404";
pub const CLIENT_SERIAL: [u8; 16] = *b"k1Pk2jcET48mxL3b";

/// High bit of the object id in `AddCharactersToScope` (and the other "enters view"
/// packets): the server sets it when the object has just appeared (spawn / teleport) and the
/// client uses it for the appear effect. The real id is `id & OBJECT_ID_MASK`.
/// This module does NOT add or remove it: `ScopeCharacter::id` travels as is.
pub const NEW_OBJECT_FLAG: u16 = 0x8000;
/// Mask to keep an object's real id.
pub const OBJECT_ID_MASK: u16 = 0x7FFF;

/// Maximum steps that fit in a walk packet (the counter is a nibble).
pub const MAX_WALK_STEPS: usize = 15;
/// Maximum characters in `CharacterList` so it fits in a C1 (8 + 7 × 34 = 246).
pub const MAX_CHARACTER_LIST: usize = 7;
/// Maximum ids in a `MapObjectOutOfScope` so it fits in a C1 (4 + 125 × 2 = 254).
pub const MAX_OUT_OF_SCOPE_IDS: usize = 125;
/// Maximum characters in an `AddCharactersToScope` (the counter is a byte).
pub const MAX_SCOPE_CHARACTERS: usize = 255;
/// `MAX_CHAT_SIZE` (WSclient.h:12): bytes of a chat message's text, with its zero.
pub const MAX_CHAT_SIZE: usize = 90;
/// Bytes of a server notice's text (without its zero) so the packet fits in a C1
/// (2 + 11 + 241 + 1 = 255), like OpenMU's `maxMessageLength` (ShowMessagePlugIn.cs).
pub const MAX_NOTICE_TEXT: usize = 241;

/// `Result` of `PRECEIVE_NOTICE` (`ReceiveNotice`, WSclient.cpp:1577-1610; `MessageType` in OpenMU).
pub mod notice_kind {
    /// Centered and golden (`CreateNotice(..., 0)`).
    pub const GOLDEN_CENTER: u8 = 0;
    /// System message, in blue.
    pub const BLUE: u8 = 1;
    /// Guild notice (`GlobalText[483]`), centered in green.
    pub const GUILD: u8 = 2;
}

/// Values of `CharacterDeleteResponse::result` (`CharacterDeleteResult` in OpenMU).
pub mod delete_result {
    /// Could not delete (the client shows it as "is in a guild").
    pub const UNSUCCESSFUL: u8 = 0;
    /// Deleted.
    pub const SUCCESSFUL: u8 = 1;
    /// Wrong security code.
    pub const WRONG_SECURITY_CODE: u8 = 2;
    /// Not in OpenMU, but the original client understands it as "items locked".
    pub const ITEM_BLOCK: u8 = 3;
}

/// Values of `Logout::kind` / `LogoutResponse::kind` (`LogOutType` in OpenMU).
pub mod logout_kind {
    pub const CLOSE_GAME: u8 = 0;
    pub const BACK_TO_CHARACTER_SELECTION: u8 = 1;
    pub const BACK_TO_SERVER_SELECTION: u8 = 2;
}

/// Actions of `AnimationRequest` / `ObjectAnimation` (`enum OptionType`, _enum.h:1498-1545: starts
/// at `AT_ATTACK1` = 120 and goes up one by one).
pub mod action {
    pub const ATTACK1: u8 = 120;
    pub const ATTACK2: u8 = 121;
    pub const STAND1: u8 = 122;
    pub const STAND2: u8 = 123;
    pub const DAMAGE1: u8 = 126;
    pub const SIT1: u8 = 128;
    pub const POSE1: u8 = 129;
    pub const HEALING1: u8 = 130;
    pub const GREETING1: u8 = 131;
    pub const GOODBYE1: u8 = 132;
    pub const CLAP1: u8 = 133;
    pub const GESTURE1: u8 = 134;
    pub const DIRECTION1: u8 = 135;
    pub const UNKNOWN1: u8 = 136;
    pub const CRY1: u8 = 137;
    pub const CHEER1: u8 = 138;
    pub const AWKWARD1: u8 = 139;
    pub const SEE1: u8 = 140;
    pub const WIN1: u8 = 141;
    pub const SMILE1: u8 = 142;
    pub const SLEEP1: u8 = 143;
    pub const COLD1: u8 = 144;
    pub const AGAIN1: u8 = 145;
    pub const RESPECT1: u8 = 146;
    pub const SALUTE1: u8 = 147;
    pub const RUSH1: u8 = 148;
    pub const SCISSORS: u8 = 149;
    pub const ROCK: u8 = 150;
    pub const PAPER: u8 = 151;
    pub const HUSTLE: u8 = 152;
    pub const PROVOCATION: u8 = 153;
    pub const LOOK_AROUND: u8 = 154;
    pub const CHEERS: u8 = 155;

    /// OpenMU's `CharacterPose` (`AnimationHandlerPlugIn`: 0x80/0x6C sitting, 0x81/0x6D leaning,
    /// 0x82/0x6E hanging), stored in the low 3 bits of appearance byte 0 and shown by the client
    /// on appearing (`Class & 0x07`, WSclient.cpp:2167-2191: 2 `PLAYER_SIT1`, 3 `PLAYER_POSE1`,
    /// 4 `PLAYER_HEALING1`). 0 = standing.
    pub fn pose(action: u8) -> u8 {
        match action {
            0x80 | 0x6C => 2,
            0x81 | 0x6D => 3,
            0x82 | 0x6E => 4,
            _ => 0,
        }
    }
}

/// Bits of `CharacterList::unlock_flags` and `CharacterClassCreationUnlock::flags`
/// (`CharacterCreationUnlockFlags` in OpenMU's `CommonEnums.xml`). 0 = nothing unlocked.
pub mod unlock {
    pub const SUMMONER: u8 = 1;
    pub const DARK_LORD: u8 = 2;
    pub const MAGIC_GLADIATOR: u8 = 4;
    pub const RAGE_FIGHTER: u8 = 8;
}

/// Login result (`LoginResult` in OpenMU).
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

    /// Text to show the player.
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
// Classes, appearance and directions
// ---------------------------------------------------------------------------------------------

/// Client class index (0 DW, 1 DK, 2 Elf, 3 MG, 4 DL, 5 Summoner, 6 RF; the original
/// client's `CLASS_*`) → OpenMU `CharacterClassNumber` without evolution.
pub fn class_number(base_class_index: u8) -> u8 {
    base_class_index << 2
}

/// `CharacterClassNumber` → client class index (0..6), ignoring evolutions.
pub fn base_class_index(class_number: u8) -> u8 {
    class_number >> 2
}

/// Appearance bytes (18) of a character of class `class` (`CharacterClassNumber`) with
/// nothing equipped, as written by OpenMU's `AppearanceSerializer` (Season 6):
/// byte 0 = `class << 3` (+ pose, 0 = standing), weapons `FF`, armor indices with all
/// bits set ("empty"), levels 0, no wings and no pet (`byte 5 |= 3`).
pub fn appearance_for_class(class: u8) -> [u8; 18] {
    Appearance::new(class).encode()
}

/// Class (`CharacterClassNumber`) from the appearance bytes (only looks at byte 0).
pub fn class_from_appearance(appearance: &[u8]) -> u8 {
    appearance.first().map_or(0, |&b| b >> 3)
}

/// MU direction table: `(dx, dy)` for each direction 0..7 of the walk packets.
///
/// It's `DirTable` from the original client's `WSclient.cpp`, used by `SendCharacterMove` to
/// convert the `PathFinding` path into directions. Matches OpenMU's `Direction`
/// minus 1 (`ToPacketByte`): 0 West, 1 SouthWest, 2 South, 3 SouthEast, 4 East, 5 NorthEast,
/// 6 North, 7 NorthWest.
pub const DIRECTIONS: [(i8, i8); 8] = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)];

/// Direction (0..7) to go from `from` to the adjacent tile `to`; `None` if not neighbours.
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

/// `CKeyGenerater::GenerateKeyValue` (KeyGenerater.cpp:39-49): the next map-move
/// key. `DWORD` with wrapping; `f[2] / f[3]` is integer division.
pub fn map_move_key(key: u32) -> u32 {
    let f = KEY_GENERATE_FILTER[(key % 10) as usize];
    key.wrapping_mul(f[0]).wrapping_add(f[1]).wrapping_sub(f[2] / f[3])
}

// ---------------------------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------------------------

/// What the client sends to the Game Server (in plain text; encrypt with `crypto::ClientCrypto`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMessage {
    Login { username: String, password: String, tick_count: u32, version: [u8; 5], serial: [u8; 16] },
    RequestCharacterList { language: u8 },
    SelectCharacter { name: String },
    /// `class` is the `CharacterClassNumber` (DW 0, DK 4, Elf 8, MG 12, DL 16, Summoner 20, RF 24).
    CreateCharacter { name: String, class: u8 },
    /// The 1.04d client always sends 20 code bytes (at most 20 here).
    DeleteCharacter { name: String, security_code: String },
    ClientReadyAfterMapChange,
    /// `directions` are directions 0..7 (see [`DIRECTIONS`]), at most [`MAX_WALK_STEPS`]
    /// (extra ones are not sent). No steps = "I stopped here facing `rotation`".
    Walk { source_x: u8, source_y: u8, rotation: u8, directions: Vec<u8> },
    Ping { tick_count: u32, attack_speed: u16, magic_speed: u16 },
    /// `kind`: see [`logout_kind`].
    Logout { kind: u8 },
    /// `SendRequestMoveMap(dwBlockKey, wMapIndex)`: `key` is the next one from [`map_move_key`] and
    /// `index` the `MOVEREQINFO::index` chosen in the move window.
    WarpCommandRequest { key: u32, index: u16 },
    /// `SendChat`: `name` is the speaker's (`Hero->ID`) and `message` the text, with the
    /// channel prefix if any (`~` party, `@` guild, `$` Gens). Bytes beyond
    /// [`MAX_CHAT_SIZE`] − 1 are cut (without splitting characters).
    PublicChat { name: String, message: String },
    /// `SendChatWhisper`: `target` is who is whispered to.
    Whisper { target: String, message: String },
    /// `SendRequestMagicTeleport(&bResult, gate, x, y)` (`EnterGateRequest`): the hero steps on the
    /// entry gate `gate` from `Gate.bmd`. `x`/`y` only matter with `gate` 0 (the wizard's
    /// teleport); when stepping on a gate they are 0.
    EnterGate { gate: u16, x: u8, y: u8 },
    /// `SendRequestAction(action, direction)` (`AnimationRequest`): `action` is an `AT_*` (see
    /// [`action`]) and `rotation` the direction 0..7 it faces.
    Animation { rotation: u8, action: u8 },
    /// `SendRequestEquipmentItem` (wsclientinline.h): move the item in `from_slot` of
    /// `from_storage` to `to_slot` of `to_storage` (see [`storage`]). `item` is the item being moved
    /// (OpenMU ignores it). Only one move may be pending: the client waits for `ItemMoved` /
    /// `ItemMoveFailed` before the next one (`EquipmentItem` flag).
    ItemMove { from_storage: u8, from_slot: u8, item: [u8; 12], to_storage: u8, to_slot: u8 },
    /// `SendRequestTalk(key)`: talk to the NPC with scope id `id`.
    TalkToNpc { id: u16 },
    /// `SendExitInventory`: close the NPC dialog (shop, vault...). Not encrypted.
    CloseNpc,
    /// `SendRequestStorageExit` (wsclientinline.h:1238-1246, C1 82, not encrypted): close the vault
    /// (`CNewUIStorageInventory::ProcessClosing`). OpenMU's `VaultClosed`.
    CloseVault,
    /// `SendRequestStorageGold(Flag, Gold)` (wsclientinline.h:1228-1236, C1 08 81, not encrypted):
    /// move `amount` zen from the inventory to the vault (`withdraw` false, `Flag` 0) or back
    /// (`Flag` 1). OpenMU's `VaultMoveMoneyRequest`; it answers with [`ServerMessage::VaultMoney`].
    VaultMoveMoney { withdraw: bool, amount: u32 },
    /// `SendStoragePassword(type, password, residentNumber)` (wsclientinline.h:1248-1255, C1 1A 83,
    /// encrypted): `kind` (see [`vault_password`]), the 4-digit `pin` and the account password (20
    /// bytes). OpenMU's `UnlockVault` / `SetVaultPin` / `RemoveVaultPin`; it answers with
    /// [`ServerMessage::VaultProtection`].
    VaultPassword { kind: u8, pin: u16, password: String },
    /// `SendRequestMixExit` (wsclientinline.h:1477-1485, C1 87, not encrypted): close the Chaos
    /// Machine (`CNewUIMixInventory::ClosingProcess`, NewUIMixInventory.cpp:136-168). OpenMU's
    /// `CraftingDialogCloseRequest`.
    CloseCrafting,
    /// `SendRequestBuy(index)`: buy the item in `slot` of the NPC store.
    BuyItem { slot: u8 },
    /// `SendRequestSell(index)`: sell the inventory item in `slot` (12-75) to the open NPC.
    SellItem { slot: u8 },
    /// `SendRequestRepair(index, addGold)`: repair `slot` (0xFF = all equipped items); `self_repair`
    /// is the inventory "repair" button (`p_AddGold` 1), not an NPC.
    RepairItem { slot: u8, self_repair: bool },
    /// `SendRequestGuildMaster(value)` (wsclientinline.h:1296-1302, C1 54, not encrypted): the answer
    /// to the guild master dialog. `false` closes it (`CNewUIGuildMakeWindow::ClosingProcess`,
    /// NewUIGuildMakeWindow.cpp:247-253). OpenMU's `GuildMasterAnswer`.
    GuildMasterAnswer { show_creation: bool },
    /// `SendRequestGensJoining(influence)` (wsclientinline.h:899-906, C1 05 F8 01, not encrypted):
    /// join gens `gens` (1 Duprian, 2 Vanert) at its steward. OpenMU's `GensJoinRequest`.
    GensJoin { gens: u8 },
    /// `SendRequestGensSecession` (wsclientinline.h:908-914, C1 04 F8 03): leave the gens.
    /// OpenMU's `GensLeaveRequest`.
    GensLeave,
    /// `SendRequestGensReward(influence)` (wsclientinline.h:916-923, C1 05 F8 09): the monthly
    /// ranking reward of gens `gens`. OpenMU's `GensRewardRequest`.
    GensReward { gens: u8 },
    /// `SendRequestQuestByNPCEPList` (wsclientinline.h:934-940, C1 04 F6 30): the quests the open
    /// NPC dialog offers. OpenMU's `AvailableQuestsRequest` (answered with `AvailableQuests`).
    AvailableQuestsRequest,
    /// `SendRequestAPDPUp` (wsclientinline.h:942-948, C1 04 F6 31): the buff of the open NPC dialog
    /// (the Elf Soldier). OpenMU's `NpcBuffRequest`.
    NpcBuffRequest,
    /// `SendQuestSelection(dwQuestIndex, byResult)` (wsclientinline.h:826-834, C1 09 F6 0A): a quest
    /// of the list was picked. `quest` is the original's `DWORD` (number in the low word, group in
    /// the high word, OpenMU's `QuestNumber` at 4 and `QuestGroup` at 6) and `selected` the 1-based
    /// line of the list. OpenMU's `QuestSelectRequest`.
    QuestSelect { quest: u32, selected: u8 },
    /// `SendQuestSelAnswer(dwQuestIndex, bySelAnswer)` (wsclientinline.h:836-844, C1 09 F6 0B): the
    /// answer (1-based) picked in the quest progress window. OpenMU's `QuestProceedRequest`
    /// (1 accepts the quest, 2 refuses it).
    QuestProceed { quest: u32, answer: u8 },
    /// `SendRequestQuestState(index, state)` (wsclientinline.h:798-804, C1 05 A2, sent encrypted):
    /// set legacy quest `quest` to `state` (1 = start or finish it). OpenMU's
    /// `LegacyQuestStateSetRequest`.
    LegacyQuestState { quest: u8, state: u8 },
    /// `SendRequestDropItem(p_InventoryIndex, p_x, p_y)` (wsclientinline.h:1135-1142, C1 23 sent
    /// encrypted, `PREQUEST_DROP_ITEM`, WSclient.h:637-643): drop inventory slot `slot` (0-11
    /// equipment, 12-75 grid) on tile (`x`, `y`). OpenMU's `DropItemRequest`; it answers with
    /// [`ServerMessage::ItemDropResult`].
    DropItem { x: u8, y: u8, slot: u8 },
    /// `SendRequestGetItem(p_Key)` (wsclientinline.h:1122-1133, C1 22 sent encrypted): pick up the
    /// ground item with scope id `id` (big endian). OpenMU's `PickupItemRequest`; it answers with
    /// [`ServerMessage::ItemPickedUp`], [`ServerMessage::InventoryMoneyUpdate`] (zen) or
    /// [`ServerMessage::ItemPickUpFailed`].
    PickUpItem { id: u16 },
}

/// `Result` of a C3 22 the client cannot use as a slot (`ReceiveGetItem`, WSclient.cpp:5603-5605;
/// OpenMU's `ItemPickUpFailReason`).
pub mod pick_up {
    /// `GET_ITEM_MULTI`: the item was stacked onto one in the inventory (OpenMU's `ItemStacked`; a
    /// durability update follows).
    pub const STACKED: u8 = 0xFD;
    /// `GET_ITEM_ZEN`: with 8 bytes it is the new zen ([`super::ServerMessage::InventoryMoneyUpdate`]);
    /// OpenMU also sends it 4 bytes long when the inventory zen limit was reached
    /// (`__MaximumInventoryMoneyReached`).
    pub const ZEN: u8 = 0xFE;
    /// `NOT_GET_ITEM`: refused (OpenMU's `General`).
    pub const FAILED: u8 = 0xFF;
}

/// A ground item entering the view (`PCREATE_ITEM`, WSclient.h:629-635: 16 bytes in C2 20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DroppedItem {
    /// Scope id of the drop (`Key & 0x7FFF`).
    pub id: u16,
    /// `CreateFlag = Key >> 15` (OpenMU's `IsFreshDrop`): it falls with a sound, otherwise it was
    /// already lying there.
    pub fresh: bool,
    pub x: u8,
    pub y: u8,
    /// The 12 item bytes; zen is `ITEM_POTION+15` with the amount in bytes 1, 2 and 4
    /// ([`DroppedItem::zen`]).
    pub item: [u8; 12],
}

/// Bytes of a [`DroppedItem`] (`sizeof(PCREATE_ITEM)` with `PACKET_ITEM_LENGTH` 12).
const DROPPED_ITEM_LEN: usize = 16;

impl DroppedItem {
    /// Dropped zen as OpenMU's `MoneyDropped` writes it (21 bytes: `MoneyNumber` 15 at 9, the
    /// amount at 10, 11 and 13 and `MoneyGroup` 14 in the high nibble of 14).
    pub fn money(id: u16, fresh: bool, x: u8, y: u8, amount: u32) -> Self {
        let mut item = [0u8; 12];
        item[0] = 15;
        item[1] = (amount >> 16) as u8;
        item[2] = (amount >> 8) as u8;
        item[4] = amount as u8;
        item[5] = 14 << 4;
        Self { id, fresh, x, y, item }
    }

    /// The amount of a zen drop: `n->Level = (Item[1]<<16)+(Item[2]<<8)+(Item[4])` when
    /// `ConvertItemType` is `ITEM_POTION+15` (`CreateItem`, ZzzObject.cpp:6111-6120).
    pub fn zen(&self) -> Option<u32> {
        let b = &self.item;
        let kind = b[0] as u32 + (b[3] as u32 & 128) * 2 + (b[5] as u32 & 240) * 32;
        (kind == 14 * 512 + 15).then(|| ((b[1] as u32) << 16) + ((b[2] as u32) << 8) + b[4] as u32)
    }
}

/// Item storages of [`ClientMessage::ItemMove`] and `ItemMoved` (`ItemStorageKind`, OpenMU
/// CommonEnums.xml; the `REQUEST_EQUIPMENT_*` source/target types of Main 5.2).
pub mod storage {
    pub const INVENTORY: u8 = 0;
    pub const TRADE: u8 = 1;
    pub const VAULT: u8 = 2;
    pub const CHAOS_MACHINE: u8 = 3;
    pub const PERSONAL_SHOP: u8 = 4;
}

/// `kind` of [`ClientMessage::VaultPassword`] (OpenMU ClientToServerPackets.xml `UnlockVault`,
/// `SetVaultPin`, `RemoveVaultPin`: the subcodes 00, 01 and 02 of C1 83).
pub mod vault_password {
    pub const UNLOCK: u8 = 0;
    pub const SET_PIN: u8 = 1;
    pub const REMOVE_PIN: u8 = 2;
}

/// `state` of [`ServerMessage::VaultProtection`] (`ProcessToReceiveStorageStatus`,
/// NewUIStorageInventory.cpp:530-596; OpenMU's `VaultProtectionState`).
pub mod vault_state {
    pub const UNPROTECTED: u8 = 0;
    pub const LOCKED: u8 = 1;
    pub const UNLOCK_FAILED_WRONG_PIN: u8 = 10;
    pub const SET_PIN_FAILED_LOCKED: u8 = 11;
    pub const UNLOCKED: u8 = 12;
    pub const REMOVE_PIN_FAILED_WRONG_PASSWORD: u8 = 13;
}

/// Vault slots: an 8×15 grid, `y · 8 + x` (`CNewUIStorageInventory`, `Create(..., 8, 15)`,
/// NewUIStorageInventory.cpp:42-43). OpenMU's extended vault (slots 120-239) has no window in the
/// Season 6 client.
pub const VAULT_SLOTS: u8 = 120;

/// Inventory slots: equipment 0-11, the 8×8 grid 12-75 (`MAX_EQUIPMENT_INDEX`, `MAX_MY_INVENTORY_INDEX`).
pub const FIRST_GRID_SLOT: u8 = 12;
pub const GRID_SLOTS: u8 = 64;

/// Window kinds of `NpcWindow` (`ReceiveTalk`, WSclient.cpp:5829: 2 vault, 3 Chaos Goblin...;
/// any value without its own `case` opens the NPC shop).
pub mod npc_window {
    pub const MERCHANT: u8 = 0;
    pub const MERCHANT1: u8 = 1;
    pub const VAULT: u8 = 2;
    pub const CHAOS_MACHINE: u8 = 3;
}

/// A character in the selection screen list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterListEntry {
    pub slot: u8,
    pub name: String,
    pub level: u16,
    /// `CharacterStatus` (0 normal, 1 banned). Only 4 bits fit in the list: it sends
    /// `status & 0x0F` (the "GameMaster" 32 doesn't fit; that goes in `CharacterInformation`).
    pub status: u8,
    /// Class (`CharacterClassNumber`) and visible equipment.
    pub appearance: Appearance,
    /// `GuildMemberRole`: 0 member, 32 battle master, 64 assistant, 128 master, 255 no guild.
    pub guild_position: u8,
}

/// Character data on entering the world (`CharacterInformation`, C3 F3 03, 72 bytes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CharacterInformation {
    pub x: u8,
    pub y: u8,
    pub map: u8,
    /// The client turns `(direction - 1) * 45` degrees.
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
    /// `CharacterHeroState`: 0 new, 1 hero, 2 light hero, 3 normal, 4 PK warning, 5/6 PK.
    pub hero_state: u8,
    /// `CharacterStatus`: 0 normal, 1 banned, 32 GM.
    pub status: u8,
    pub used_fruit_points: u16,
    pub max_fruit_points: u16,
    pub leadership: u16,
    pub used_negative_fruit_points: u16,
    pub max_negative_fruit_points: u16,
    pub inventory_extensions: u8,
}

/// A player entering the view area (`AddCharactersToScope`, S6 variant with effects).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeCharacter {
    /// Object id as it travels on the wire: the server may add [`NEW_OBJECT_FLAG`]
    /// if it just appeared; the client must keep `id & OBJECT_ID_MASK`.
    pub id: u16,
    pub x: u8,
    pub y: u8,
    /// Class (`CharacterClassNumber`), pose and visible equipment.
    pub appearance: Appearance,
    pub name: String,
    pub target_x: u8,
    pub target_y: u8,
    /// High nibble of byte 34 (the client turns `(rotation - 1) * 45` degrees).
    pub rotation: u8,
    /// `CharacterHeroState`, low nibble of byte 34.
    pub hero_state: u8,
}

/// A monster or NPC entering the view area (`PCREATE_MONSTER` from `ReceiveCreateMonsterViewport`,
/// WSclient.cpp:2512-2639), without effects (`s_BuffCount` 0).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeMonster {
    /// Object id as it travels on the wire: [`NEW_OBJECT_FLAG`] = just appeared (`AppearMonster`).
    pub id: u16,
    /// Monster number (`Type`, 10 bits: `TypeH & 0x03` and `TypeL`).
    pub number: u16,
    pub x: u8,
    pub y: u8,
    /// Tile it is heading to (the client walks there with `PathFinding2`).
    pub target_x: u8,
    pub target_y: u8,
    /// `Path >> 4`: the client turns `(rotation - 1) * 45` degrees.
    pub rotation: u8,
}

/// Bytes of a [`ScopeMonster`] without effects.
const SCOPE_MONSTER_LEN: usize = 10;

impl ScopeMonster {
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.id.to_be_bytes());
        out.push(((self.number >> 8) & 0x03) as u8);
        out.push(self.number as u8);
        out.extend_from_slice(&[self.x, self.y, self.target_x, self.target_y, self.rotation << 4, 0]);
    }

    fn read(d: &[u8], i: usize) -> Option<(Self, usize)> {
        let e = d.get(i..i + SCOPE_MONSTER_LEN)?;
        let m = Self {
            id: u16::from_be_bytes([e[0], e[1]]),
            number: (((e[2] & 0x03) as u16) << 8) | e[3] as u16,
            x: e[4],
            y: e[5],
            target_x: e[6],
            target_y: e[7],
            rotation: e[8] >> 4,
        };
        Some((m, i + SCOPE_MONSTER_LEN + e[9] as usize))
    }
}

/// What the Game Server sends to the client (already decrypted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMessage {
    /// Monsters and NPCs entering view (`0x13`). At most [`MAX_SCOPE_CHARACTERS`].
    AddMonstersToScope(Vec<ScopeMonster>),
    GameServerEntered { success: bool, player_id: u16, version: [u8; 5] },
    LoginResponse(LoginResult),
    /// At most [`MAX_CHARACTER_LIST`] characters (extra ones are not sent).
    /// `unlock_flags`: see [`unlock`].
    CharacterList { unlock_flags: u8, move_count: u8, vault_extended: bool, characters: Vec<CharacterListEntry> },
    /// `flags`: see [`unlock`].
    CharacterClassCreationUnlock { flags: u8 },
    CharacterCreated { name: String, slot: u8, level: u16, class: u8 },
    CharacterCreationFailed,
    /// `result`: see [`delete_result`] (0 failure, 1 deleted, 2 wrong code).
    CharacterDeleteResponse { result: u8 },
    CharacterInformation(CharacterInformation),
    /// At most [`MAX_SCOPE_CHARACTERS`]; sent without effects.
    AddCharactersToScope(Vec<ScopeCharacter>),
    /// In S6 the client only uses the destination and rotation (it recomputes the path with
    /// `PathFinding2`); `directions` may be empty. If present, it is packed as in `Walk`.
    ObjectWalked { id: u16, target_x: u8, target_y: u8, rotation: u8, directions: Vec<u8> },
    /// At most [`MAX_OUT_OF_SCOPE_IDS`] ids (extra ones are not sent).
    MapObjectOutOfScope(Vec<u16>),
    LogoutResponse { kind: u8 },
    /// The map-move key (`PMSG_MAPMOVE_CHECKSUM`).
    MapMoveChecksumKey { key: u32 },
    /// Map change or teleport (`PRECEIVE_TELEPORT_POSITION`). `map` is `WorldActive`
    /// (folder `World{map + 1}`); the client turns `(rotation - 1) * 45` degrees.
    MapChanged { is_map_change: bool, map: u16, x: u8, y: u8, rotation: u8 },
    /// Inventory zen (`ReceiveGetItem` with `Result` = `GET_ITEM_ZEN` = FE, WSclient.cpp:5607).
    InventoryMoneyUpdate { money: u32 },
    /// Someone nearby spoke (`ReceiveChat`); the text carries the channel prefix.
    ChatMessage { sender: String, message: String },
    /// A whisper for us (`ReceiveChatWhisper`).
    WhisperMessage { sender: String, message: String },
    /// Result of the last whisper (`ReceiveChatWhisperResult`): `false` = nobody with
    /// that name.
    WhisperResult { success: bool },
    /// Server notice (`ReceiveNotice`); `kind`: see [`notice_kind`]. The scrolling-banner
    /// fields (`Count`, `Delay`, `Color`, `Speed`, kinds 10-15) are zero. At most
    /// [`MAX_NOTICE_TEXT`] bytes of text.
    Notice { kind: u8, text: String },
    /// Someone plays an animation (`ReceiveAction`): `direction` as in `MapChanged`, `action` an
    /// `AT_*` (see [`action`]) and `target` the target id (0 if none).
    ObjectAnimation { id: u16, direction: u8, action: u8, target: u16 },
    /// An object in view (the own player included) moved instantly (`ReceiveMovePosition`, C1 15,
    /// WSclient.cpp:1673): OpenMU also sends it to pull the client back when it rejects a walk.
    ObjectMoved { id: u16, x: u8, y: u8 },
    /// The whole inventory after entering the world (`ReceiveInventory`, C4 F3 10): slot and the 12
    /// item bytes of each item (equipment 0-11, grid 12-75, personal shop 204+).
    InventoryList(Vec<(u8, [u8; 12])>),
    /// `ReceiveEquipmentItem` (0x24) with `SubCode` != 255: the pending move was done and the item is
    /// now in `slot` of `storage` (see [`storage`]).
    ItemMoved { storage: u8, slot: u8, item: [u8; 12] },
    /// `ReceiveEquipmentItem` with `SubCode` 255: the move was refused (`BackupPickedItem`).
    ItemMoveFailed,
    /// `ReceiveDeleteInventory` (0x28): the item in `slot` is gone (used up, stacked...).
    ItemRemoved { slot: u8 },
    /// `ReceiveDurability` (0x2A): new durability (or stack size) of the item in `slot`.
    ItemDurability { slot: u8, durability: u8, by_consumption: bool },
    /// `ReceiveTalk` (C3 30): the NPC dialog opens; `window` (see [`npc_window`]) says which one.
    NpcWindow { window: u8 },
    /// `ReceiveTradeInventory` (C2 31): items of the NPC store or of the vault (`kind` 0), with their
    /// slot in that storage.
    StoreItemList { kind: u8, items: Vec<(u8, [u8; 12])> },
    /// `ReceiveBuy` (C1 32) with a slot: the bought item went to inventory slot `slot`.
    ItemBought { slot: u8, item: [u8; 12] },
    /// `ReceiveBuy` with `Index` 255 (or 254, "store closed"): the purchase failed.
    BuyFailed { closed: bool },
    /// `ReceiveSell` (C3 33, `PRECEIVE_GOLD`): `result` 0 = refused (the item goes back), 0xFF =
    /// refused with a message, 0xFE = refused and the store closes, anything else = sold; `money` is
    /// the zen after the sale (OpenMU sends 0 or 1).
    SellResult { result: u8, money: u32 },
    /// `ReceiveStorageGold` (C1 0C 81, `PRECEIVE_STORAGE_GOLD`, WSclient.h:1180-1186 and
    /// WSclient.cpp:6463-6473): with `success` the vault holds `vault` zen and the inventory
    /// `inventory`. OpenMU's `VaultMoneyUpdate`, sent when the vault opens and after every
    /// [`ClientMessage::VaultMoveMoney`] (always with `success`, the values unchanged if refused).
    VaultMoney { success: bool, vault: u32, inventory: u32 },
    /// `ReceiveStorageExit` (C1 03 82, WSclient.cpp:6475-6478): the server confirms the vault closed
    /// (the original only logs it). OpenMU's `VaultClosed`.
    VaultClosed,
    /// `ReceiveStorageStatus` (C1 04 83, WSclient.cpp:6480-6485): the vault's lock `state` (see
    /// [`vault_state`]). OpenMU's `VaultProtectionInformation`, sent when the vault opens.
    VaultProtection { state: u8 },
    /// `ReceiveChangePlayer` (C1 25, `PCHANGE_CHARACTER`, WSclient.cpp:1871): a player in view changed
    /// what it shows in `slot` (see [`appearance::slot`]); `None` = took it off. Apply it with
    /// [`Appearance::apply_change`]. OpenMU's `AppearanceChanged`.
    AppearanceChanged { id: u16, slot: u8, item: Option<VisibleItem> },
    /// `ReceiveNPCDlgUIStart` (C3 F9 01, WSclient.cpp:9881-9890, `PMSG_ANS_NPC_CLICK`): the dialog
    /// of an NPC with talk lines (the Gens NPCs...) opens. OpenMU's `OpenNpcDialog` (12 bytes: the NPC
    /// number at 4, little endian, and the gens contribution points at 8).
    OpenNpcDialog { npc: u16, contribution: u32 },
    /// `ReceiveQuestState` (C1 A1, WSclient.cpp:9539-9546): the quest NPC window opens with the state
    /// of quest `quest` (`state` holds four quests). OpenMU's `LegacyQuestStateDialog`.
    QuestStateDialog { quest: u8, state: u8 },
    /// `ReceiveCreateGuildInterface` (C1 54, WSclient.cpp:6700-6703): the guild master window opens.
    /// OpenMU's `ShowGuildMasterDialog`.
    GuildMasterDialog,
    /// `ReceiveGensJoining` (C1 F8 02, WSclient.cpp:9830-9835, `PMSG_ANS_REG_GENS_MEMBER`): the
    /// answer to [`ClientMessage::GensJoin`]; `result` 0 = joined `gens`. OpenMU's `GensJoinResponse`.
    GensJoinResult { result: u8, gens: u8 },
    /// `ReceiveGensSecession` (C1 F8 04, WSclient.cpp:9837-9842, `PMSG_ANS_SECEDE_GENS_MEMBER`): the
    /// answer to [`ClientMessage::GensLeave`]; 0 = left. OpenMU's `GensLeaveResponse`.
    GensLeaveResult { result: u8 },
    /// `ReceiveReward` (C1 F8 0A, WSclient.cpp:9893-9899, `PMSG_GENS_REWARD_CODE`): the answer to
    /// [`ClientMessage::GensReward`]. OpenMU's `GensRewardResponse`.
    GensRewardResult { result: u8 },
    /// `ReceivePlayerGensInfluence` (C1 F8 07, WSclient.cpp:9844-9852, `PMSG_MSG_SEND_GENS_INFO`):
    /// the own gens (`m_byInfluence` at 4: 0 none, 1 Duprian, 2 Vanert), ranking (8), rank (12),
    /// contribution (16) and the contribution of the next rank (20). OpenMU's `GensInfo`.
    GensInfo { gens: u8, ranking: u32, rank: u32, contribution: u32, next_contribution: u32 },
    /// `ReceiveQuestByNPCEPList` (C1 F6 0A, WSclient.cpp:9742-9748, `PMSG_NPCTALK_QUESTLIST`): the
    /// quests the NPC offers (`m_wNPCIndex` at 4, `m_wQuestCount` at 6, then one `DWORD` each:
    /// number in the low word, group in the high word). OpenMU's `AvailableQuests`.
    AvailableQuests { npc: u16, quests: Vec<u32> },
    /// `ReceiveQuestQSSelSentence` (C1 F6 0B, WSclient.cpp:9750-9755, `PMSG_NPC_QUESTEXP_INFO`): the
    /// quest step to show (`m_dwQuestIndex` at 4: step number in the low word, group in the high
    /// word). OpenMU's `QuestStepInfo`.
    QuestStepInfo { quest: u32 },
    /// `ReceiveQuestQSRequestReward` (C1 F6 0C, WSclient.cpp:9757-9763, `PMSG_NPC_QUESTEXP_INFO`):
    /// the requests and rewards of a running quest step (`m_dwQuestIndex` at 4, as in
    /// [`ServerMessage::QuestStepInfo`]). OpenMU's `QuestProgress` (251 bytes: the counts at 8-10,
    /// 5 conditions of 26 bytes from 11 and 5 rewards of 22 from 141); only the step is decoded yet.
    QuestRequestReward { quest: u32 },
    /// `ReceiveChatKey` (C1 01, WSclient.cpp:1553-1575, `PCHATING_KEY`): `message` over the head of
    /// the object with scope id `id` (an NPC too: `CreateChat`). OpenMU's `ObjectMessage`, e.g. the
    /// quest NPC's "I have no quests for you.".
    ObjectMessage { id: u16, message: String },
    /// `ReceiveCreateItemViewport` (C2 20, WSclient.cpp:5558-5586): items (and zen) lying on the
    /// ground that came into view. OpenMU's `ItemsDropped` and `MoneyDropped` (one zen drop per packet).
    ItemsDropped(Vec<DroppedItem>),
    /// `ReceiveDeleteItemViewport` (C2 21, WSclient.cpp:5588-5601): these ground items are gone
    /// (picked up, timed out or out of view). OpenMU's `ItemDropRemoved`.
    ItemDropRemoved(Vec<u16>),
    /// `ReceiveDropItem` (C1 23, `PHEADER_DEFAULT_KEY`, WSclient.cpp:5663-5685): the answer to
    /// [`ClientMessage::DropItem`]; `KeyH` = `success` and `KeyL` the inventory `slot`. OpenMU's
    /// `ItemDropResponse`.
    ItemDropResult { success: bool, slot: u8 },
    /// `ReceiveGetItem` (C3 22, `PRECEIVE_GET_ITEM`, WSclient.cpp:5607-5661) with a slot: the picked
    /// up item went to inventory slot `slot`. OpenMU's `ItemAddedToInventory`.
    ItemPickedUp { slot: u8, item: [u8; 12] },
    /// `ReceiveGetItem` with `Result` 0xFF (refused) or 0xFD (stacked onto an inventory item), or the
    /// 4-byte 0xFE of OpenMU's zen limit (see [`pick_up`]). OpenMU's `ItemPickUpRequestFailed`.
    ItemPickUpFailed { reason: u8 },
}

// ---------------------------------------------------------------------------------------------
// (De)serialization utilities
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

/// The first bytes of `s` that fit in `max` without splitting a UTF-8 character.
pub fn truncate_utf8(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// `<name 10> <text> 00` after `code` (`PCHATING`): the text at most `max − 1` bytes.
fn chat_packet(code: u8, name: &str, text: &str, max: usize) -> Vec<u8> {
    let text = truncate_utf8(text, max - 1);
    let mut body = Vec::with_capacity(1 + 10 + text.len() + 1);
    body.push(code);
    body.extend_from_slice(&fixed::<10>(name));
    body.extend_from_slice(text.as_bytes());
    body.push(0);
    c1(&body)
}

/// Reads `<name 10> <text>` from a `PCHATING` (the text ends at its zero or at the end).
fn read_chat(d: &[u8]) -> Option<(String, String)> {
    Some((unfixed(d.get(3..13)?), unfixed(d.get(13..).unwrap_or(&[]))))
}

/// Fixed-size C1 packet of `len` with code and subcode; the rest zeroed.
fn fixed_c1(len: usize, code: u8, sub_code: u8) -> Vec<u8> {
    debug_assert!(len <= 255);
    let mut p = vec![0u8; len];
    p[0] = 0xC1;
    p[1] = len as u8;
    p[2] = code;
    p[3] = sub_code;
    p
}

/// Same as [`fixed_c1`] but with a C3 header (the encryption layer encrypts it).
fn fixed_c3(len: usize, code: u8, sub_code: u8) -> Vec<u8> {
    let mut p = fixed_c1(len, code, sub_code);
    p[0] = 0xC3;
    p
}

/// The same packet marked to go encrypted (C1 → C3, C2 → C4).
fn encrypted(mut p: Vec<u8>) -> Vec<u8> {
    p[0] = if p[0] == 0xC2 { 0xC4 } else { 0xC3 };
    p
}

/// `[n] n × {slot, 12 item bytes}` from `i` (inventory, store and vault lists).
fn read_item_list(d: &[u8], i: usize) -> Option<Vec<(u8, [u8; 12])>> {
    let count = *d.get(i)? as usize;
    (0..count)
        .map(|k| {
            let e = d.get(i + 1 + k * 13..i + 1 + (k + 1) * 13)?;
            Some((e[0], e[1..13].try_into().ok()?))
        })
        .collect()
}

fn write_item_list(body: &mut Vec<u8>, items: &[(u8, [u8; 12])]) {
    let items = &items[..items.len().min(255)];
    body.push(items.len() as u8);
    for (slot, item) in items {
        body.push(*slot);
        body.extend_from_slice(item);
    }
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

/// `<rotation:4 | steps:4>` followed by the steps, two per byte (the first in the high nibble),
/// like the original client's `SendCharacterMove` and OpenMU's `DecodePayload`.
fn pack_steps(out: &mut Vec<u8>, rotation: u8, directions: &[u8]) {
    let directions = &directions[..directions.len().min(MAX_WALK_STEPS)];
    out.push((rotation << 4) | directions.len() as u8);
    for pair in directions.chunks(2) {
        let low = pair.get(1).map_or(0, |d| d & 0x0F);
        out.push(((pair[0] & 0x0F) << 4) | low);
    }
}

/// Inverse of [`pack_steps`]: `p[i]` is the rotation/steps byte.
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

/// Packet bytes if it has a 1-byte size header (C1/C3).
fn short_header<'a>(p: &Packet<'a>) -> Option<&'a [u8]> {
    (p.header.size_field_len() == 1).then_some(p.bytes)
}

/// Packet bytes if it has a 2-byte size header (C2/C4).
fn long_header<'a>(p: &Packet<'a>) -> Option<&'a [u8]> {
    (p.header.size_field_len() == 2).then_some(p.bytes)
}

const CHARACTER_LIST_ENTRY_LEN: usize = 34;
const SCOPE_CHARACTER_LEN: usize = 36; // without effects
const CHARACTER_INFORMATION_LEN: usize = 72;
const CHARACTER_CREATED_LEN: usize = 42;
const MAP_CHANGED_LEN: usize = 15;

// ---------------------------------------------------------------------------------------------
// Client → server
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
                body.push(0); // padding: the original packet is 60 long (the serial ends at 59)
                let mut p = c1(&body);
                p[0] = 0xC3; // sent encrypted
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
            Self::PublicChat { name, message } => chat_packet(0x00, name, message, MAX_CHAT_SIZE),
            Self::Whisper { target, message } => chat_packet(0x02, target, message, MAX_CHAT_SIZE),
            Self::EnterGate { gate, x, y } => {
                // `spe.AddNullData(1)` at byte 3; `Send(TRUE)`: encrypted (C3).
                let mut p = fixed_c3(8, 0x1C, 0x00);
                put_u16_le(&mut p, 4, *gate);
                p[6] = *x;
                p[7] = *y;
                p
            }
            Self::Animation { rotation, action } => c1(&[0x18, *rotation, *action]),
            // `spe.Send(TRUE)`: encrypted (C3), except `SendExitInventory`.
            Self::ItemMove { from_storage, from_slot, item, to_storage, to_slot } => {
                let mut body = vec![0x24, *from_storage, *from_slot];
                body.extend_from_slice(item);
                body.extend_from_slice(&[*to_storage, *to_slot]);
                encrypted(c1(&body))
            }
            Self::TalkToNpc { id } => {
                let [h, l] = id.to_be_bytes();
                encrypted(c1(&[0x30, h, l]))
            }
            Self::CloseNpc => c1(&[0x31]),
            Self::CloseVault => c1(&[0x82]),
            // `spe << (BYTE)Flag << (DWORD)Gold; spe.Send()`.
            Self::VaultMoveMoney { withdraw, amount } => {
                let mut p = fixed_c1(8, 0x81, *withdraw as u8);
                put_u32_le(&mut p, 4, *amount);
                p
            }
            // `spe << (BYTE)type << (WORD)password; spe.AddData(residentNumber, 20); spe.Send(TRUE)`.
            Self::VaultPassword { kind, pin, password } => {
                let mut p = fixed_c3(26, 0x83, *kind);
                put_u16_le(&mut p, 4, *pin);
                p[6..26].copy_from_slice(&fixed::<20>(password));
                p
            }
            Self::CloseCrafting => c1(&[0x87]),
            Self::BuyItem { slot } => encrypted(c1(&[0x32, *slot])),
            Self::SellItem { slot } => encrypted(c1(&[0x33, *slot])),
            Self::RepairItem { slot, self_repair } => encrypted(c1(&[0x34, *slot, *self_repair as u8])),
            Self::GuildMasterAnswer { show_creation } => c1(&[0x54, *show_creation as u8]),
            // `spe.Send()`: not encrypted.
            Self::GensJoin { gens } => c1(&[0xF8, 0x01, *gens]),
            Self::GensLeave => c1(&[0xF8, 0x03]),
            Self::GensReward { gens } => c1(&[0xF8, 0x09, *gens]),
            Self::AvailableQuestsRequest => c1(&[0xF6, 0x30]),
            Self::NpcBuffRequest => c1(&[0xF6, 0x31]),
            Self::QuestSelect { quest, selected } => {
                let mut body = vec![0xF6, 0x0A];
                body.extend_from_slice(&quest.to_le_bytes());
                body.push(*selected);
                c1(&body)
            }
            Self::QuestProceed { quest, answer } => {
                let mut body = vec![0xF6, 0x0B];
                body.extend_from_slice(&quest.to_le_bytes());
                body.push(*answer);
                c1(&body)
            }
            // `spe.Send(TRUE)`.
            Self::LegacyQuestState { quest, state } => encrypted(c1(&[0xA2, *quest, *state])),
            // `spe << x << y << index; spe.Send(TRUE)`.
            Self::DropItem { x, y, slot } => encrypted(c1(&[0x23, *x, *y, *slot])),
            Self::PickUpItem { id } => {
                let [h, l] = id.to_be_bytes();
                encrypted(c1(&[0x22, h, l]))
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let p = Packet::parse(bytes)?;
        let d = short_header(&p)?;
        // Packets without a subcode.
        match p.code() {
            // Byte 3 is the null padding (OpenMU also accepts it unencrypted).
            0x1C => return Some(Self::EnterGate { gate: get_u16_le(d, 4)?, x: *d.get(6)?, y: *d.get(7)? }),
            0x18 => return Some(Self::Animation { rotation: *d.get(3)?, action: *d.get(4)? }),
            0xD4 => {
                let (rotation, directions) = unpack_steps(d, 5)?;
                return Some(Self::Walk { source_x: *d.get(3)?, source_y: *d.get(4)?, rotation, directions });
            }
            0x00 => {
                let (name, message) = read_chat(d)?;
                return Some(Self::PublicChat { name, message });
            }
            0x02 => {
                let (target, message) = read_chat(d)?;
                return Some(Self::Whisper { target, message });
            }
            0x24 if d.len() >= 19 => {
                return Some(Self::ItemMove {
                    from_storage: d[3],
                    from_slot: d[4],
                    item: d[5..17].try_into().ok()?,
                    to_storage: d[17],
                    to_slot: d[18],
                });
            }
            0x30 => return Some(Self::TalkToNpc { id: get_u16_be(d, 3)? }),
            0x31 => return Some(Self::CloseNpc),
            0x82 => return Some(Self::CloseVault),
            0x81 => return Some(Self::VaultMoveMoney { withdraw: *d.get(3)? != 0, amount: get_u32_le(d, 4)? }),
            0x83 => {
                return Some(Self::VaultPassword { kind: *d.get(3)?, pin: get_u16_le(d, 4)?, password: unfixed(d.get(6..26)?) });
            }
            0x87 => return Some(Self::CloseCrafting),
            0x32 => return Some(Self::BuyItem { slot: *d.get(3)? }),
            0x33 => return Some(Self::SellItem { slot: *d.get(3)? }),
            0x34 => return Some(Self::RepairItem { slot: *d.get(3)?, self_repair: *d.get(4).unwrap_or(&0) != 0 }),
            0x54 => return Some(Self::GuildMasterAnswer { show_creation: *d.get(3)? != 0 }),
            0xA2 => return Some(Self::LegacyQuestState { quest: *d.get(3)?, state: *d.get(4)? }),
            0x23 => return Some(Self::DropItem { x: *d.get(3)?, y: *d.get(4)?, slot: *d.get(5)? }),
            0x22 => return Some(Self::PickUpItem { id: get_u16_be(d, 3)? }),
            _ => {}
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
                // Some clients send fewer than 20 bytes (only the 7 of the code).
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
            (0xF8, 0x01) => Some(Self::GensJoin { gens: *d.get(4)? }),
            (0xF8, 0x03) => Some(Self::GensLeave),
            (0xF8, 0x09) => Some(Self::GensReward { gens: *d.get(4)? }),
            (0xF6, 0x30) => Some(Self::AvailableQuestsRequest),
            (0xF6, 0x31) => Some(Self::NpcBuffRequest),
            (0xF6, 0x0A) => Some(Self::QuestSelect { quest: get_u32_le(d, 4)?, selected: *d.get(8)? }),
            (0xF6, 0x0B) => Some(Self::QuestProceed { quest: get_u32_le(d, 4)?, answer: *d.get(8)? }),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Server → client
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
        // 50..52: padding (the zen DWORD is 4-aligned in the client's struct).
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
        // e[11]: padding (the level WORD is 2-aligned).
        put_u16_le(e, 12, self.level);
        e[14] = self.status & 0x0F; // high nibble: IsItemBlockActive = false
        e[15..33].copy_from_slice(&self.appearance.encode());
        e[33] = self.guild_position;
    }

    fn read(e: &[u8]) -> Option<Self> {
        let e = e.get(..CHARACTER_LIST_ENTRY_LEN)?;
        Some(Self {
            slot: e[0],
            name: unfixed(&e[1..11]),
            level: get_u16_le(e, 12)?,
            status: e[14] & 0x0F,
            appearance: Appearance::decode(e[15..33].try_into().ok()?),
            guild_position: e[33],
        })
    }
}

impl ScopeCharacter {
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.id.to_be_bytes());
        out.push(self.x);
        out.push(self.y);
        out.extend_from_slice(&self.appearance.encode());
        out.extend_from_slice(&fixed::<10>(&self.name));
        out.push(self.target_x);
        out.push(self.target_y);
        out.push((self.rotation << 4) | (self.hero_state & 0x0F));
        out.push(0); // without effects
    }

    /// Reads a character at `d[i..]`; also returns where the next one starts.
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
            appearance: Appearance::decode(e[4..22].try_into().ok()?),
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
                p[19] = 0; // normal CharacterStatus
                // PreviewData (20..42): OpenMU fills it entirely with FF and the client ignores it
                // (it uses the class); here goes the appearance without equipment and the rest FF.
                p[20..38].copy_from_slice(&appearance_for_class(*class));
                p[38..42].fill(0xFF);
                p
            }
            Self::CharacterCreationFailed => fixed_c1(5, 0xF3, 0x01),
            Self::CharacterDeleteResponse { result } => c1(&[0xF3, 0x02, *result]),
            Self::CharacterInformation(info) => info.encode(),
            Self::AddMonstersToScope(monsters) => {
                let monsters = &monsters[..monsters.len().min(MAX_SCOPE_CHARACTERS)];
                let mut body = Vec::with_capacity(2 + monsters.len() * SCOPE_MONSTER_LEN);
                body.push(0x13);
                body.push(monsters.len() as u8);
                for m in monsters {
                    m.write(&mut body);
                }
                c2(&body)
            }
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
                // OpenMU's `MapChanged`: 15 bytes, byte 3 is 0F and 10 to 14 are zero.
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
            Self::ChatMessage { sender, message } => chat_packet(0x00, sender, message, MAX_CHAT_SIZE),
            Self::WhisperMessage { sender, message } => chat_packet(0x02, sender, message, MAX_CHAT_SIZE),
            Self::WhisperResult { success } => c1(&[0x0C, *success as u8]),
            Self::Notice { kind, text } => {
                // `Result`, `Count`, padding, `Delay`, `Color` and `Speed` (bytes 3..13) and the text.
                let text = truncate_utf8(text, MAX_NOTICE_TEXT);
                let mut body = vec![0u8; 11];
                body[0] = 0x0D;
                body[1] = *kind;
                body.extend_from_slice(text.as_bytes());
                body.push(0);
                c1(&body)
            }
            Self::ObjectAnimation { id, direction, action, target } => {
                let [ih, il] = id.to_be_bytes();
                let [th, tl] = target.to_be_bytes();
                c1(&[0x18, ih, il, *direction, *action, th, tl])
            }
            // OpenMU `ObjectMoved`: 8 bytes, the last one unused.
            Self::ObjectMoved { id, x, y } => {
                let [h, l] = id.to_be_bytes();
                c1(&[0x15, h, l, *x, *y, 0])
            }
            // OpenMU `CharacterInventory`: C4 F3 10 <n> n × {slot, item}.
            Self::InventoryList(items) => {
                let mut body = vec![0xF3, 0x10];
                write_item_list(&mut body, items);
                encrypted(c2(&body))
            }
            // `ItemMoved` / `ItemMoveRequestFailed` (17 bytes, `[3]` = 0xFF when it failed).
            Self::ItemMoved { storage, slot, item } => {
                let mut body = vec![0x24, *storage, *slot];
                body.extend_from_slice(item);
                encrypted(c1(&body))
            }
            Self::ItemMoveFailed => {
                let mut body = vec![0x24, 0xFF, 0];
                body.extend_from_slice(&[0; 12]);
                encrypted(c1(&body))
            }
            Self::ItemRemoved { slot } => c1(&[0x28, *slot, 1]),
            Self::ItemDurability { slot, durability, by_consumption } => c1(&[0x2A, *slot, *durability, *by_consumption as u8]),
            // `NpcWindowResponse` measures 11; only the window byte means something.
            Self::NpcWindow { window } => encrypted(c1(&[0x30, *window, 0, 0, 0, 0, 0, 0, 0])),
            Self::StoreItemList { kind, items } => {
                let mut body = vec![0x31, *kind];
                write_item_list(&mut body, items);
                c2(&body)
            }
            Self::ItemBought { slot, item } => {
                let mut body = vec![0x32, *slot];
                body.extend_from_slice(item);
                c1(&body)
            }
            Self::BuyFailed { closed } => c1(&[0x32, if *closed { 0xFE } else { 0xFF }]),
            Self::SellResult { result, money } => {
                let mut p = encrypted(c1(&[0x33, *result, 0, 0, 0, 0]));
                put_u32_le(&mut p, 4, *money);
                p
            }
            Self::VaultMoney { success, vault, inventory } => {
                let mut p = fixed_c1(12, 0x81, *success as u8);
                put_u32_le(&mut p, 4, *vault);
                put_u32_le(&mut p, 8, *inventory);
                p
            }
            Self::VaultClosed => c1(&[0x82]),
            Self::VaultProtection { state } => c1(&[0x83, *state]),
            Self::AppearanceChanged { id, slot, item } => {
                let mut body = vec![0x25];
                body.extend_from_slice(&id.to_be_bytes());
                body.extend_from_slice(&appearance::encode_changed_item(*slot as usize, *item));
                c1(&body)
            }
            Self::OpenNpcDialog { npc, contribution } => {
                let mut p = fixed_c3(12, 0xF9, 0x01);
                put_u16_le(&mut p, 4, *npc);
                put_u32_le(&mut p, 8, *contribution);
                p
            }
            Self::QuestStateDialog { quest, state } => c1(&[0xA1, *quest, *state]),
            Self::GuildMasterDialog => c1(&[0x54]),
            Self::GensJoinResult { result, gens } => c1(&[0xF8, 0x02, *result, *gens]),
            Self::GensLeaveResult { result } => c1(&[0xF8, 0x04, *result]),
            Self::GensRewardResult { result } => c1(&[0xF8, 0x0A, *result]),
            Self::GensInfo { gens, ranking, rank, contribution, next_contribution } => {
                let mut p = fixed_c1(24, 0xF8, 0x07);
                p[4] = *gens;
                put_u32_le(&mut p, 8, *ranking);
                put_u32_le(&mut p, 12, *rank);
                put_u32_le(&mut p, 16, *contribution);
                put_u32_le(&mut p, 20, *next_contribution);
                p
            }
            Self::AvailableQuests { npc, quests } => {
                let mut body = vec![0xF6, 0x0A];
                body.extend_from_slice(&npc.to_le_bytes());
                body.extend_from_slice(&(quests.len() as u16).to_le_bytes());
                for q in quests {
                    body.extend_from_slice(&q.to_le_bytes());
                }
                c1(&body)
            }
            Self::QuestStepInfo { quest } => {
                let mut body = vec![0xF6, 0x0B];
                body.extend_from_slice(&quest.to_le_bytes());
                // OpenMU's packet is 11 long; the rest of `PMSG_NPC_QUESTEXP_INFO` is unused here.
                body.extend_from_slice(&[0, 0, 0]);
                c1(&body)
            }
            Self::QuestRequestReward { quest } => {
                let mut p = fixed_c1(251, 0xF6, 0x0C);
                put_u32_le(&mut p, 4, *quest);
                p
            }
            Self::ObjectMessage { id, message } => {
                let mut body = vec![0x01];
                body.extend_from_slice(&id.to_be_bytes());
                body.extend_from_slice(truncate_utf8(message, MAX_CHAT_SIZE).as_bytes());
                body.push(0);
                c1(&body)
            }
            Self::ItemsDropped(items) => {
                let items = &items[..items.len().min(255)];
                let mut body = Vec::with_capacity(2 + items.len() * DROPPED_ITEM_LEN);
                body.extend_from_slice(&[0x20, items.len() as u8]);
                for i in items {
                    body.extend_from_slice(&((i.id & OBJECT_ID_MASK) | if i.fresh { NEW_OBJECT_FLAG } else { 0 }).to_be_bytes());
                    body.extend_from_slice(&[i.x, i.y]);
                    body.extend_from_slice(&i.item);
                }
                c2(&body)
            }
            Self::ItemDropRemoved(ids) => {
                let ids = &ids[..ids.len().min(255)];
                let mut body = vec![0x21, ids.len() as u8];
                for id in ids {
                    body.extend_from_slice(&id.to_be_bytes());
                }
                c2(&body)
            }
            Self::ItemDropResult { success, slot } => c1(&[0x23, *success as u8, *slot]),
            Self::ItemPickedUp { slot, item } => {
                let mut body = vec![0x22, *slot];
                body.extend_from_slice(item);
                encrypted(c1(&body))
            }
            Self::ItemPickUpFailed { reason } => encrypted(c1(&[0x22, *reason])),
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let p = Packet::parse(bytes)?;
        // Packets without a subcode.
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
            0x13 => {
                let d = long_header(&p)?;
                let count = *d.get(4)? as usize;
                let mut monsters = Vec::with_capacity(count);
                let mut i = 5;
                for _ in 0..count {
                    let (m, next) = ScopeMonster::read(d, i)?;
                    monsters.push(m);
                    i = next;
                }
                return Some(Self::AddMonstersToScope(monsters));
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
            0x00 => {
                let (sender, message) = read_chat(short_header(&p)?)?;
                return Some(Self::ChatMessage { sender, message });
            }
            0x02 => {
                let (sender, message) = read_chat(short_header(&p)?)?;
                return Some(Self::WhisperMessage { sender, message });
            }
            // `PHEADER_DEFAULT::Value`.
            0x0C => return Some(Self::WhisperResult { success: *short_header(&p)?.get(3)? != 0 }),
            0x0D => {
                let d = short_header(&p)?;
                return Some(Self::Notice { kind: *d.get(3)?, text: unfixed(d.get(13..).unwrap_or(&[])) });
            }
            // `PRECEIVE_ACTION`: `KeyH`, `KeyL`, `Angle`, `Action`, `TargetKeyH`, `TargetKeyL`.
            0x18 => {
                let d = short_header(&p)?;
                return Some(Self::ObjectAnimation {
                    id: get_u16_be(d, 3)?,
                    direction: *d.get(5)?,
                    action: *d.get(6)?,
                    target: get_u16_be(d, 7).unwrap_or(0),
                });
            }
            // `PRECEIVE_MOVE_POSITION`: `KeyH`, `KeyL`, `PositionX`, `PositionY`.
            0x15 => {
                let d = short_header(&p)?;
                return Some(Self::ObjectMoved { id: get_u16_be(d, 3)?, x: *d.get(5)?, y: *d.get(6)? });
            }
            0xF3 if long_header(&p).is_some() => {
                let d = long_header(&p)?;
                return match *d.get(4)? {
                    0x10 => Some(Self::InventoryList(read_item_list(d, 5)?)),
                    _ => None,
                };
            }
            // `PHEADER_DEFAULT_SUBCODE_ITEM`: `SubCode` (storage, 255 = failed), `Index`, `Item`.
            0x24 => {
                let d = short_header(&p)?;
                return Some(match *d.get(3)? {
                    0xFF => Self::ItemMoveFailed,
                    storage => Self::ItemMoved { storage, slot: *d.get(4)?, item: d.get(5..17)?.try_into().ok()? },
                });
            }
            0x28 => return Some(Self::ItemRemoved { slot: *short_header(&p)?.get(3)? }),
            0x2A => {
                let d = short_header(&p)?;
                return Some(Self::ItemDurability {
                    slot: *d.get(3)?,
                    durability: *d.get(4)?,
                    by_consumption: d.get(5).is_some_and(|&b| b != 0),
                });
            }
            0x30 => return Some(Self::NpcWindow { window: *short_header(&p)?.get(3)? }),
            0x31 => {
                let d = long_header(&p)?;
                return Some(Self::StoreItemList { kind: *d.get(4)?, items: read_item_list(d, 5)? });
            }
            // `PHEADER_DEFAULT_ITEM`: 255 failed, 254 failed and the store closes.
            0x32 => {
                let d = short_header(&p)?;
                return Some(match *d.get(3)? {
                    0xFF => Self::BuyFailed { closed: false },
                    0xFE => Self::BuyFailed { closed: true },
                    slot => Self::ItemBought { slot, item: d.get(4..16)?.try_into().ok()? },
                });
            }
            0x33 => {
                let d = short_header(&p)?;
                return Some(Self::SellResult { result: *d.get(3)?, money: get_u32_le(d, 4).unwrap_or(0) });
            }
            // `PRECEIVE_STORAGE_GOLD`: `Result`, `StorageGold` and `Gold` (DWORDs at 4 and 8).
            0x81 => {
                let d = short_header(&p)?;
                return Some(Self::VaultMoney { success: *d.get(3)? != 0, vault: get_u32_le(d, 4)?, inventory: get_u32_le(d, 8)? });
            }
            0x82 => return Some(Self::VaultClosed),
            // `PHEADER_DEFAULT::Value`.
            0x83 => return Some(Self::VaultProtection { state: *short_header(&p)?.get(3)? }),
            // `PCHANGE_CHARACTER`: `KeyH`, `KeyL`, `Item[PACKET_ITEM_LENGTH]`.
            0x25 => {
                let d = short_header(&p)?;
                let (slot, item) = appearance::decode_changed_item(d.get(5..17)?.try_into().ok()?);
                return Some(Self::AppearanceChanged { id: get_u16_be(d, 3)?, slot: slot as u8, item });
            }
            // `PRECEIVE_QUEST_STATE`: `m_byQuestIndex`, `m_byState`.
            0xA1 => {
                let d = short_header(&p)?;
                return Some(Self::QuestStateDialog { quest: *d.get(3)?, state: *d.get(4)? });
            }
            0x54 => return Some(Self::GuildMasterDialog),
            // `PWHEADER_DEFAULT_WORD` (`Value` = count at 4) and `PCREATE_ITEM`s from 5.
            0x20 => {
                let d = long_header(&p)?;
                let count = *d.get(4)? as usize;
                let items = (0..count)
                    .map(|k| {
                        let e = d.get(5 + k * DROPPED_ITEM_LEN..5 + (k + 1) * DROPPED_ITEM_LEN)?;
                        let key = u16::from_be_bytes([e[0], e[1]]);
                        Some(DroppedItem {
                            id: key & OBJECT_ID_MASK,
                            fresh: key & NEW_OBJECT_FLAG != 0,
                            x: e[2],
                            y: e[3],
                            item: e[4..16].try_into().ok()?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                return Some(Self::ItemsDropped(items));
            }
            // `PWHEADER_DEFAULT_WORD` and `PDELETE_CHARACTER`s (`KeyH`, `KeyL`).
            0x21 => {
                let d = long_header(&p)?;
                let count = *d.get(4)? as usize;
                let ids = (0..count).map(|k| get_u16_be(d, 5 + 2 * k)).collect::<Option<Vec<u16>>>()?;
                return Some(Self::ItemDropRemoved(ids));
            }
            // `PHEADER_DEFAULT_KEY`: `KeyH` = success, `KeyL` = slot.
            0x23 => {
                let d = short_header(&p)?;
                return Some(Self::ItemDropResult { success: *d.get(3)? != 0, slot: *d.get(4)? });
            }
            // `PRECEIVE_GET_ITEM`: `Result` and `Item[0..12]`.
            0x22 => {
                let d = short_header(&p)?;
                return Some(match *d.get(3)? {
                    // `Item[0..4]` in big endian.
                    pick_up::ZEN if d.len() >= 8 => {
                        Self::InventoryMoneyUpdate { money: u32::from_be_bytes(d.get(4..8)?.try_into().ok()?) }
                    }
                    reason @ (pick_up::STACKED | pick_up::ZEN | pick_up::FAILED) => Self::ItemPickUpFailed { reason },
                    slot => Self::ItemPickedUp { slot, item: d.get(4..16)?.try_into().ok()? },
                });
            }
            // `PCHATING_KEY`: `KeyH`, `KeyL`, `ChatText`.
            0x01 => {
                let d = short_header(&p)?;
                return Some(Self::ObjectMessage { id: get_u16_be(d, 3)?, message: unfixed(d.get(5..).unwrap_or(&[])) });
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
                // 0 = failure; the original client also understands 2 as another failure.
                _ => Some(Self::CharacterCreationFailed),
            },
            (0xF3, 0x02) => Some(Self::CharacterDeleteResponse { result: *d.get(4)? }),
            (0xF3, 0x03) => Some(Self::CharacterInformation(CharacterInformation::decode(d)?)),
            (0xDE, 0x00) => Some(Self::CharacterClassCreationUnlock { flags: *d.get(4)? }),
            (0x8E, 0x01) => Some(Self::MapMoveChecksumKey { key: get_u32_le(d, 4)? }),
            // `ReceiveTeleport` doesn't look at byte 3 (nor the length: `MapChangedExtended` is 16 long).
            (0x1C, _) if d.len() >= 10 => Some(Self::MapChanged {
                // The original's `WORD Flag` (bytes 4 and 5); with S6 maps (< 256) byte 5
                // (map high byte) is 0 and byte 4 is enough, like OpenMU's `IsMapChange`.
                is_map_change: d[4] != 0,
                map: get_u16_be(d, 5)?,
                x: d[7],
                y: d[8],
                rotation: d[9],
            }),
            (0xF9, 0x01) => Some(Self::OpenNpcDialog { npc: get_u16_le(d, 4)?, contribution: get_u32_le(d, 8)? }),
            (0xF8, 0x02) => Some(Self::GensJoinResult { result: *d.get(4)?, gens: *d.get(5).unwrap_or(&0) }),
            (0xF8, 0x04) => Some(Self::GensLeaveResult { result: *d.get(4)? }),
            (0xF8, 0x0A) => Some(Self::GensRewardResult { result: *d.get(4)? }),
            (0xF8, 0x07) => Some(Self::GensInfo {
                gens: *d.get(4)?,
                ranking: get_u32_le(d, 8)?,
                rank: get_u32_le(d, 12)?,
                contribution: get_u32_le(d, 16)?,
                next_contribution: get_u32_le(d, 20)?,
            }),
            (0xF6, 0x0A) => {
                let count = get_u16_le(d, 6)? as usize;
                // The original reads `m_wQuestCount` DWORDs; a short packet is cut to what it holds.
                let quests = (0..count).map_while(|i| get_u32_le(d, 8 + 4 * i)).collect();
                Some(Self::AvailableQuests { npc: get_u16_le(d, 4)?, quests })
            }
            (0xF6, 0x0B) => Some(Self::QuestStepInfo { quest: get_u32_le(d, 4)? }),
            (0xF6, 0x0C) => Some(Self::QuestRequestReward { quest: get_u32_le(d, 4)? }),
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
        // User at index 4, password at 14, tick at 34, version at 38, serial at 43.
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
        // Original client table: 0 = (-1,-1), 3 = (+1, 0), 7 = (-1, 0).
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
            CharacterListEntry { slot: 0, name: "Mago".into(), level: 400, status: 0, appearance: Appearance::new(3), guild_position: 255 },
            CharacterListEntry { slot: 1, name: "Elfa".into(), level: 1, status: 1, appearance: Appearance::new(8), guild_position: 128 },
            CharacterListEntry { slot: 4, name: "Lord".into(), level: 250, status: 0, appearance: Appearance::new(17), guild_position: 0 },
        ];
        let msg = ServerMessage::CharacterList { unlock_flags: 0x0F, move_count: 3, vault_extended: true, characters };
        let wire = ida_y_vuelta_servidor(msg);
        assert_eq!(wire.len(), 8 + 3 * 34);
        assert_eq!(&wire[..8], &[0xC1, (8 + 3 * 34) as u8, 0xF3, 0x00, 0x0F, 3, 3, 1]);
        // First character: LE level at 8+12, appearance at 8+15 (Grand Master = 3 << 3).
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
        // Rotation in the high nibble, steps in the low one; then two steps per byte, the first on top.
        assert_eq!(walk, [0xC1, 8, 0xD4, 130, 125, 0x53, 0x34, 0x50]);

        let parado = ida_y_vuelta_cliente(ClientMessage::Walk { source_x: 10, source_y: 20, rotation: 2, directions: vec![] });
        assert_eq!(parado, [0xC1, 6, 0xD4, 10, 20, 0x20]);

        let largo = ClientMessage::Walk { source_x: 1, source_y: 1, rotation: 0, directions: vec![1; 20] };
        match ClientMessage::decode(&largo.encode()) {
            Some(ClientMessage::Walk { directions, .. }) => assert_eq!(directions.len(), MAX_WALK_STEPS),
            otro => panic!("{otro:?}"),
        }

        // The original client always sends 13 bytes (Path[8]) even with fewer steps.
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
    fn monstruos_que_entran_en_vista() {
        let bichos = vec![
            ScopeMonster { id: 0x8000 | 5, number: 3, x: 180, y: 90, target_x: 182, target_y: 91, rotation: 2 },
            // 10-bit number (S6 ones go above 255).
            ScopeMonster { id: 0x0102, number: 0x0215, x: 1, y: 2, target_x: 1, target_y: 2, rotation: 7 },
        ];
        let wire = ida_y_vuelta_servidor(ServerMessage::AddMonstersToScope(bichos));
        assert_eq!(wire.len(), 5 + 2 * 10);
        assert_eq!(&wire[..5], &[0xC2, 0, 25, 0x13, 2]);
        assert_eq!(&wire[5..15], &[0x80, 5, 0, 3, 180, 90, 182, 91, 0x20, 0]);
        assert_eq!(&wire[15..19], &[0x01, 0x02, 0x02, 0x15]);

        // With effects (`s_BuffCount` 2) the next one starts after them.
        let paquete = c2(&[0x13, 2, 0, 1, 0, 3, 10, 10, 10, 10, 0x10, 2, 7, 8, 0, 2, 0, 4, 20, 20, 20, 20, 0x30, 0]);
        match ServerMessage::decode(&paquete) {
            Some(ServerMessage::AddMonstersToScope(v)) => {
                assert_eq!(v.len(), 2);
                assert_eq!((v[1].id, v[1].number, v[1].rotation), (2, 4, 3));
            }
            otro => panic!("{otro:?}"),
        }
    }

    #[test]
    fn jugadores_que_entran_y_salen_de_vista() {
        let jugadores = vec![
            ScopeCharacter {
                id: 0x8000 | 123,
                x: 130,
                y: 125,
                appearance: Appearance::new(4),
                name: "Guerrero".into(),
                target_x: 132,
                target_y: 127,
                rotation: 3,
                hero_state: 3,
            },
            ScopeCharacter {
                id: 456,
                x: 1,
                y: 2,
                appearance: Appearance::new(20),
                name: "Invocadora".into(),
                target_x: 1,
                target_y: 2,
                rotation: 0,
                hero_state: 0,
            },
        ];
        let wire = ida_y_vuelta_servidor(ServerMessage::AddCharactersToScope(jugadores));
        assert_eq!(wire.len(), 5 + 2 * 36);
        assert_eq!(&wire[..5], &[0xC2, 0, (5 + 2 * 36) as u8, 0x12, 2]);
        assert_eq!(&wire[5..7], &[0x80, 123], "el id viaja tal cual, con su bit alto");
        assert_eq!(&wire[9..27], &appearance_for_class(4));
        assert_eq!(wire[5 + 34], 0x33);
        assert_eq!(wire[5 + 35], 0, "sin efectos");
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::AddCharactersToScope(vec![])).len(), 5);

        // A character with effects (as OpenMU sends them) is also read.
        let mut con_efectos = wire[3..5 + 36].to_vec();
        con_efectos[1] = 1;
        con_efectos[2 + 35] = 2;
        con_efectos.extend_from_slice(&[10, 20]);
        let paquete = c2(&con_efectos);
        match ServerMessage::decode(&paquete) {
            Some(ServerMessage::AddCharactersToScope(v)) => assert_eq!(v[0].name, "Guerrero"),
            otro => panic!("{otro:?}"),
        }

        // Sitting (`CharacterPose::Sitting` = 2): in appearance byte 0, below the class.
        let sentado = ScopeCharacter { id: 9, appearance: Appearance { pose: 2, ..Appearance::new(8) }, name: "Elfa".into(), ..Default::default() };
        let wire = ida_y_vuelta_servidor(ServerMessage::AddCharactersToScope(vec![sentado]));
        assert_eq!(wire[9], (8 << 3) | 2);

        let fuera = ida_y_vuelta_servidor(ServerMessage::MapObjectOutOfScope(vec![1, 0x1234, 0x7FFF]));
        assert_eq!(fuera, [0xC1, 10, 0x14, 3, 0, 1, 0x12, 0x34, 0x7F, 0xFF]);
        let muchos = ServerMessage::MapObjectOutOfScope((0..300).collect()).encode();
        assert_eq!(muchos.len(), 4 + 2 * MAX_OUT_OF_SCOPE_IDS);
    }

    #[test]
    fn apariencia_con_equipo_en_la_lista_y_en_vista() {
        // A Blade Knight with gear as OpenMU's `AppearanceSerializer` writes it (appearance.rs tests).
        let bytes = [0x30, 0x0E, 0x09, 0x18, 0x3F, 0x5B, 0x04, 0x7D, 0x44, 0xB3, 0xA0, 0x61, 0x04, 0xC0, 0x11, 0xF0, 0x00, 0x01];
        let appearance = Appearance::decode(&bytes);
        assert_eq!(appearance.class, 6);
        assert_eq!(appearance.equipment[appearance::slot::WEAPON_RIGHT].map(|w| (w.group, w.index, w.level)), Some((0, 14, 9)));
        let entry = CharacterListEntry { slot: 0, name: "Caballero".into(), level: 380, status: 0, appearance, guild_position: 255 };
        let wire = ida_y_vuelta_servidor(ServerMessage::CharacterList {
            unlock_flags: 0,
            move_count: 0,
            vault_extended: false,
            characters: vec![entry],
        });
        assert_eq!(&wire[8 + 15..8 + 33], &bytes);
        let scope = ScopeCharacter { id: 7, appearance, name: "Caballero".into(), ..Default::default() };
        let wire = ida_y_vuelta_servidor(ServerMessage::AddCharactersToScope(vec![scope]));
        assert_eq!(&wire[9..27], &bytes);
    }

    #[test]
    fn cambio_de_apariencia() {
        // OpenMU's `AppearanceChanged`: player 0x1234 equips a Kris +9 (excellent bit 0x02, durability 42)
        // in slot 0; `Item[1]` carries `slot << 4 | glow level` (AppearanceChangedPlugIn.cs:62-63).
        let kris = [0xC1, 0x11, 0x25, 0x12, 0x34, 0x05, 0x04, 0x2A, 0x02, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(
            ServerMessage::decode(&kris),
            Some(ServerMessage::AppearanceChanged {
                id: 0x1234,
                slot: 0,
                item: Some(VisibleItem { group: 0, index: 5, level: 9, excellent: 2, ancient: 0 }),
            })
        );
        // Wings taken off: the item is all FF except `Item[1]`.
        let sin_alas = ida_y_vuelta_servidor(ServerMessage::AppearanceChanged { id: 0x1234, slot: 7, item: None });
        let mut esperado = vec![0xC1, 0x11, 0x25, 0x12, 0x34];
        esperado.extend_from_slice(&[0xFF, 0x70, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(sin_alas, esperado);
        let armadura = VisibleItem { group: 8, index: 40, level: 13, excellent: 1, ancient: 5 };
        ida_y_vuelta_servidor(ServerMessage::AppearanceChanged { id: 3, slot: 3, item: Some(armadura) });
    }

    #[test]
    fn clave_de_mover() {
        // By hand with `GenerateKeyValue`: 0 % 10 = 0 → 0 * 321 + 37531879 - 8734 / 32 (= 272).
        assert_eq!(map_move_key(0), 37_531_879 - 272);
        // 1234567 % 10 = 7 → 1234567 * 665 + 78641332 - 6547 / 54 (= 121).
        assert_eq!(map_move_key(1_234_567), 1_234_567u32.wrapping_mul(665) + 78_641_332 - 121);
        // DWORD: wraps without failing (u32::MAX % 10 = 5 → -987 + 24654876 - 5616 / 54).
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
        // `MapChangedExtended` (16, with the discriminator) is also read.
        let mut extendido = cambio.clone();
        extendido[1] = 16;
        extendido.push(7);
        assert_eq!(
            ServerMessage::decode(&extendido),
            Some(ServerMessage::MapChanged { is_map_change: true, map: 3, x: 175, y: 112, rotation: 1 })
        );

        let zen = ida_y_vuelta_servidor(ServerMessage::InventoryMoneyUpdate { money: 0x0001_E240 });
        assert_eq!(zen, [0xC3, 8, 0x22, 0xFE, 0x00, 0x01, 0xE2, 0x40], "zen en big endian");
        // Encrypted end to end.
        let mut client = ClientCrypto::default();
        let mut server = ServerCrypto::default();
        for m in [ServerMessage::MapChanged { is_map_change: true, map: 51, x: 52, y: 225, rotation: 0 }, ServerMessage::InventoryMoneyUpdate { money: 5 }] {
            assert_eq!(ServerMessage::decode(&client.decrypt(&server.encrypt(&m.encode())).unwrap()), Some(m));
        }
        let w = ClientMessage::WarpCommandRequest { key: 9, index: 42 };
        assert_eq!(ClientMessage::decode(&server.decrypt(&client.encrypt(&w.encode())).unwrap()), Some(w));
    }

    #[test]
    fn puertas_a_pie() {
        let pedir = ida_y_vuelta_cliente(ClientMessage::EnterGate { gate: 1, x: 0, y: 0 });
        // `spe.Init(0xC1, 0x1C)`, `AddNullData(1)`, `WORD` gate, x, y; encrypted (C3), 8 like OpenMU.
        assert_eq!(pedir, [0xC3, 8, 0x1C, 0x00, 1, 0, 0, 0]);
        let mago = ida_y_vuelta_cliente(ClientMessage::EnterGate { gate: 0x0123, x: 130, y: 125 });
        assert_eq!(&mago[4..], &[0x23, 0x01, 130, 125]);
        // Unencrypted (C1) is also read, like `WarpGateHandlerPlugIn` (`IsEncryptionExpected` false).
        assert_eq!(ClientMessage::decode(&[0xC1, 8, 0x1C, 0, 3, 0, 0, 0]), Some(ClientMessage::EnterGate { gate: 3, x: 0, y: 0 }));
        assert_eq!(ClientMessage::decode(&[0xC3, 6, 0x1C, 0, 3, 0]), None, "corto");
        // Encrypted end to end.
        let mut client = ClientCrypto::default();
        let mut server = ServerCrypto::default();
        let m = ClientMessage::EnterGate { gate: 3, x: 0, y: 0 };
        let wire = client.encrypt(&m.encode());
        assert_eq!(wire[0], 0xC3);
        assert_eq!(ClientMessage::decode(&server.decrypt(&wire).unwrap()), Some(m));
    }

    #[test]
    fn animaciones() {
        let sentarse = ida_y_vuelta_cliente(ClientMessage::Animation { rotation: 5, action: action::SIT1 });
        // `spe << (BYTE)p_Angle << (BYTE)p_Action`.
        assert_eq!(sentarse, [0xC1, 5, 0x18, 5, 0x80]);
        let visto = ida_y_vuelta_servidor(ServerMessage::ObjectAnimation { id: 0x1234, direction: 5, action: action::SIT1, target: 0 });
        // OpenMU's `ObjectAnimation` (9): BE id at 3, direction at 5, action at 6, target at 7.
        assert_eq!(visto, [0xC1, 9, 0x18, 0x12, 0x34, 5, 0x80, 0, 0]);
        ida_y_vuelta_servidor(ServerMessage::ObjectAnimation { id: 7, direction: 0, action: action::GREETING1, target: 0x0102 });
        assert_eq!((action::SIT1, action::POSE1, action::HEALING1, action::GREETING1, action::RUSH1, action::CHEERS), (128, 129, 130, 131, 148, 155));
        assert_eq!([action::pose(0x80), action::pose(0x81), action::pose(0x82), action::pose(action::GREETING1)], [2, 3, 4, 0]);
        let mut client = ClientCrypto::default();
        let mut server = ServerCrypto::default();
        let m = ClientMessage::Animation { rotation: 1, action: action::POSE1 };
        assert_eq!(ClientMessage::decode(&server.decrypt(&client.encrypt(&m.encode())).unwrap()), Some(m));
    }

    #[test]
    fn chat_y_susurros() {
        let hablar = ida_y_vuelta_cliente(ClientMessage::PublicChat { name: "Guerrero".into(), message: "hola".into() });
        // `spe.Init(0xC1, 0x00)`, `Hero->ID` in 10 bytes and the text with its zero.
        assert_eq!(hablar.len(), 3 + 10 + 5);
        assert_eq!(&hablar[..3], &[0xC1, 18, 0x00]);
        assert_eq!(&hablar[3..13], b"Guerrero\0\0");
        assert_eq!(&hablar[13..], b"hola\0");

        let susurro = ida_y_vuelta_cliente(ClientMessage::Whisper { target: "Elfa".into(), message: "~grupo ñ".into() });
        assert_eq!(&susurro[..3], &[0xC1, susurro.len() as u8, 0x02]);
        assert_eq!(*susurro.last().unwrap(), 0);

        // At most `MAX_CHAT_SIZE` bytes of text with its zero, without splitting characters.
        let largo = ClientMessage::PublicChat { name: "A".into(), message: "x".repeat(200) }.encode();
        assert_eq!(largo.len(), 3 + 10 + MAX_CHAT_SIZE);
        let tildes = ClientMessage::PublicChat { name: "A".into(), message: "é".repeat(60) }.encode();
        match ClientMessage::decode(&tildes) {
            Some(ClientMessage::PublicChat { message, .. }) => assert_eq!(message, "é".repeat(44), "88 bytes: 44 letras"),
            otro => panic!("{otro:?}"),
        }

        // The original copies `MAX_CHAT_SIZE` bytes even if the packet is shorter: the zero is enough.
        let del_original = [0xC1, 3 + 10 + 4, 0x00, b'P', b'e', b'p', b'e', 0, 0, 0, 0, 0, 0, b'h', b'o', b'l', 0];
        assert_eq!(
            ClientMessage::decode(&del_original),
            Some(ClientMessage::PublicChat { name: "Pepe".into(), message: "hol".into() })
        );

        let visto = ida_y_vuelta_servidor(ServerMessage::ChatMessage { sender: "Mago".into(), message: "@gremio".into() });
        assert_eq!(&visto[..3], &[0xC1, visto.len() as u8, 0x00]);
        let susurrado = ida_y_vuelta_servidor(ServerMessage::WhisperMessage { sender: "Mago".into(), message: "psst".into() });
        assert_eq!(&susurrado[2..7], &[0x02, b'M', b'a', b'g', b'o']);
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::WhisperResult { success: false }), [0xC1, 4, 0x0C, 0]);
        ida_y_vuelta_servidor(ServerMessage::WhisperResult { success: true });
        // Empty: nothing but the zero.
        ida_y_vuelta_servidor(ServerMessage::ChatMessage { sender: "Mago".into(), message: String::new() });

        // Encrypted end to end (the client passes C1 through Xor32).
        let mut client = ClientCrypto::default();
        let mut server = ServerCrypto::default();
        let m = ClientMessage::PublicChat { name: "Pepe".into(), message: "¿qué tal?".into() };
        assert_eq!(ClientMessage::decode(&server.decrypt(&client.encrypt(&m.encode())).unwrap()), Some(m));
        let r = ServerMessage::WhisperMessage { sender: "Pepe".into(), message: "bien".into() };
        assert_eq!(ServerMessage::decode(&client.decrypt(&server.encrypt(&r.encode())).unwrap()), Some(r));
    }

    #[test]
    fn avisos_del_servidor() {
        let aviso = ida_y_vuelta_servidor(ServerMessage::Notice { kind: notice_kind::BLUE, text: "Hola".into() });
        // `PRECEIVE_NOTICE` without `pack`: kind at 3 and the text at 13.
        assert_eq!(aviso.len(), 13 + 5);
        assert_eq!(&aviso[..4], &[0xC1, 18, 0x0D, 1]);
        assert_eq!(&aviso[4..13], &[0; 9]);
        assert_eq!(&aviso[13..], b"Hola\0");
        // As OpenMU sends it to S6: the text at 4 after nine "0".
        let openmu = c1(&[&[0x0D, 1][..], b"000000000", b"Aviso", &[0]].concat());
        assert_eq!(ServerMessage::decode(&openmu), Some(ServerMessage::Notice { kind: 1, text: "Aviso".into() }));
        // Fits in a C1 even if the text is long.
        let largo = ServerMessage::Notice { kind: notice_kind::GOLDEN_CENTER, text: "a".repeat(400) }.encode();
        assert_eq!(largo.len(), 255);
        assert_eq!(truncate_utf8("añb", 2), "a", "no parte la ñ");
        assert_eq!(truncate_utf8("añb", 3), "añ");
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

        // An unencrypted C1 (walking) also goes through the client's encryption layer (Xor32).
        let walk = ClientMessage::Walk { source_x: 1, source_y: 2, rotation: 3, directions: vec![4, 5] };
        assert_eq!(ClientMessage::decode(&server.decrypt(&client.encrypt(&walk.encode())).unwrap()), Some(walk));
    }

    /// A +9 Kris with skill and luck (group 0, index 1), as OpenMU's `ItemSerializer` writes it.
    const KRIS: [u8; 12] = [1, 0x80 | 9 << 3 | 4, 30, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

    #[test]
    fn item_and_npc_requests() {
        // ItemMoveRequest: 19 bytes, from (storage, slot), the item, to (storage, slot); encrypted.
        let mv = ClientMessage::ItemMove { from_storage: storage::INVENTORY, from_slot: 12, item: KRIS, to_storage: storage::INVENTORY, to_slot: 0 };
        let wire = ida_y_vuelta_cliente(mv);
        assert_eq!(wire.len(), 19);
        assert_eq!(&wire[..5], &[0xC3, 19, 0x24, 0, 12]);
        assert_eq!(&wire[5..17], &KRIS);
        assert_eq!(&wire[17..], &[0, 0]);

        assert_eq!(ida_y_vuelta_cliente(ClientMessage::TalkToNpc { id: 0x1234 }), [0xC3, 5, 0x30, 0x12, 0x34]);
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::CloseNpc), [0xC1, 3, 0x31]);
        // `spe.Init(0xC1, 0x82); spe.Send()` and `spe.Init(0xC1, 0x87); spe.Send()`: header only.
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::CloseVault), [0xC1, 3, 0x82]);
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::CloseCrafting), [0xC1, 3, 0x87]);
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::BuyItem { slot: 16 }), [0xC3, 4, 0x32, 16]);
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::SellItem { slot: 20 }), [0xC3, 4, 0x33, 20]);
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::RepairItem { slot: 0xFF, self_repair: true }), [0xC3, 5, 0x34, 0xFF, 1]);
    }

    #[test]
    fn object_moved() {
        // OpenMU `ObjectMoved` (C1 08 15): id big endian, x, y and one unused byte.
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::ObjectMoved { id: 0x0200, x: 125, y: 124 }), [0xC1, 8, 0x15, 2, 0, 125, 124, 0]);
    }

    #[test]
    fn inventory_and_npc_answers() {
        // CharacterInventory (OpenMU UpdateInventoryListPlugIn): C4 len F3 10 <n> n × {slot, item}.
        let mut wire = vec![0xC4, 0, 6 + 2 * 13, 0xF3, 0x10, 2, 0];
        wire.extend_from_slice(&KRIS);
        wire.push(12);
        wire.extend_from_slice(&KRIS);
        let list = ServerMessage::decode(&wire).unwrap();
        assert_eq!(list, ServerMessage::InventoryList(vec![(0, KRIS), (12, KRIS)]));
        assert_eq!(list.encode(), wire);

        // StoreItemList (merchant and vault): C2 len 31 <kind> <n> n × {slot, item}.
        let mut wire = vec![0xC2, 0, 6 + 13, 0x31, 0, 1, 16];
        wire.extend_from_slice(&KRIS);
        assert_eq!(ServerMessage::decode(&wire), Some(ServerMessage::StoreItemList { kind: 0, items: vec![(16, KRIS)] }));

        // ItemMoved / ItemMoveRequestFailed: 17 bytes, [3] storage or 0xFF.
        let moved = ida_y_vuelta_servidor(ServerMessage::ItemMoved { storage: storage::INVENTORY, slot: 0, item: KRIS });
        assert_eq!((moved.len(), &moved[..5]), (17, &[0xC3, 17, 0x24, 0, 0][..]));
        let failed = ida_y_vuelta_servidor(ServerMessage::ItemMoveFailed);
        assert_eq!((failed.len(), failed[3]), (17, 0xFF));

        assert_eq!(ida_y_vuelta_servidor(ServerMessage::ItemRemoved { slot: 20 }), [0xC1, 5, 0x28, 20, 1]);
        assert_eq!(
            ida_y_vuelta_servidor(ServerMessage::ItemDurability { slot: 20, durability: 3, by_consumption: true }),
            [0xC1, 6, 0x2A, 20, 3, 1]
        );
        // NpcWindowResponse measures 11 and only [3] matters (OpenMU leaves the rest uncleared).
        let window = ida_y_vuelta_servidor(ServerMessage::NpcWindow { window: npc_window::VAULT });
        assert_eq!((window.len(), &window[..4]), (11, &[0xC3, 11, 0x30, 2][..]));
        assert_eq!(ServerMessage::decode(&[0xC3, 11, 0x30, 0, 9, 9, 9, 9, 9, 9, 9]), Some(ServerMessage::NpcWindow { window: 0 }));

        // ItemBought (16 bytes) or NpcItemBuyFailed (C1 04 32 FF).
        let bought = ida_y_vuelta_servidor(ServerMessage::ItemBought { slot: 12, item: KRIS });
        assert_eq!((bought.len(), &bought[..4]), (16, &[0xC1, 16, 0x32, 12][..]));
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::BuyFailed { closed: false }), [0xC1, 4, 0x32, 0xFF]);
        // NpcItemSellResult: success and the total money, little endian.
        assert_eq!(
            ida_y_vuelta_servidor(ServerMessage::SellResult { result: 1, money: 0x0001_E240 }),
            [0xC3, 8, 0x33, 1, 0x40, 0xE2, 0x01, 0x00]
        );
    }

    /// The vault, with bytes captured from OpenMU (the `--baul` bot, 2026-11-24): talking to the Safety
    /// Guardian sends `NpcWindowResponse` 2, `StoreItemList` 0 with the vault items,
    /// `VaultMoneyUpdate` and `VaultProtectionInformation`.
    #[test]
    fn vault_packets() {
        // VaultMoneyUpdate: success, the vault's zen and the inventory's (10 000 000), little endian.
        let open = [0xC1, 0x0C, 0x81, 0x01, 0, 0, 0, 0, 0x80, 0x96, 0x98, 0x00];
        let msg = ServerMessage::VaultMoney { success: true, vault: 0, inventory: 10_000_000 };
        assert_eq!(ServerMessage::decode(&open), Some(msg.clone()));
        assert_eq!(msg.encode(), open);
        // After depositing 1 zen (preceded by `InventoryMoneyUpdate`, C3 08 22 FE 00 98 96 7F).
        let deposited = [0xC1, 0x0C, 0x81, 0x01, 0x01, 0, 0, 0, 0x7F, 0x96, 0x98, 0x00];
        assert_eq!(ServerMessage::decode(&deposited), Some(ServerMessage::VaultMoney { success: true, vault: 1, inventory: 9_999_999 }));
        assert_eq!(
            ServerMessage::decode(&[0xC3, 0x08, 0x22, 0xFE, 0x00, 0x98, 0x96, 0x7F]),
            Some(ServerMessage::InventoryMoneyUpdate { money: 9_999_999 })
        );
        // VaultProtectionInformation: unprotected.
        assert_eq!(ServerMessage::decode(&[0xC1, 0x04, 0x83, 0x00]), Some(ServerMessage::VaultProtection { state: vault_state::UNPROTECTED }));
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::VaultProtection { state: vault_state::LOCKED }), [0xC1, 4, 0x83, 1]);
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::VaultClosed), [0xC1, 3, 0x82]);
        // The vault list: kind 0, the vault slot (`y · 8 + x`) and the item.
        let mut list = vec![0xC2, 0x00, 0x20, 0x31, 0x00, 0x02, 0x21];
        list.extend_from_slice(&[0x13, 0x38, 0x01, 0x00, 0x00, 0xE0, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        list.push(0x25);
        list.extend_from_slice(&[0x13, 0x28, 0x01, 0x00, 0x00, 0xE0, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        let Some(ServerMessage::StoreItemList { kind: 0, items }) = ServerMessage::decode(&list) else { panic!("vault list") };
        assert_eq!(items.iter().map(|(s, _)| *s).collect::<Vec<_>>(), [33, 37]);

        // VaultMoveMoneyRequest: deposit 1 and withdraw 2 000 000 000 (`SendRequestStorageGold`).
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::VaultMoveMoney { withdraw: false, amount: 1 }), [0xC1, 8, 0x81, 0, 1, 0, 0, 0]);
        assert_eq!(
            ida_y_vuelta_cliente(ClientMessage::VaultMoveMoney { withdraw: true, amount: 2_000_000_000 }),
            [0xC1, 8, 0x81, 1, 0x00, 0x94, 0x35, 0x77]
        );
        // A move into vault slot 119 and its answer.
        let to_vault = ClientMessage::ItemMove {
            from_storage: storage::INVENTORY,
            from_slot: 12,
            item: [0x0D, 0x00, 0x01, 0x00, 0x00, 0xE0, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            to_storage: storage::VAULT,
            to_slot: 119,
        };
        assert_eq!(
            ida_y_vuelta_cliente(to_vault),
            [0xC3, 0x13, 0x24, 0x00, 0x0C, 0x0D, 0x00, 0x01, 0x00, 0x00, 0xE0, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x02, 0x77]
        );
        let moved = [0xC3, 0x11, 0x24, 0x02, 0x77, 0x0D, 0x00, 0x01, 0x00, 0x00, 0xE0, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(
            ServerMessage::decode(&moved),
            Some(ServerMessage::ItemMoved { storage: storage::VAULT, slot: 119, item: moved[5..17].try_into().unwrap() })
        );
        // An item OpenMU does not store: a notice, then the refusal.
        let refused = [0xC3, 0x11, 0x24, 0xFF, 0x00, 0x14, 0x08, 0x1E, 0x00, 0x00, 0xD0, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(ServerMessage::decode(&refused), Some(ServerMessage::ItemMoveFailed));

        // `SendStoragePassword` (26 bytes, encrypted): unlock with pin 1234.
        let unlock = ida_y_vuelta_cliente(ClientMessage::VaultPassword { kind: vault_password::UNLOCK, pin: 1234, password: String::new() });
        assert_eq!((unlock.len(), &unlock[..6]), (26, &[0xC3, 26, 0x83, 0, 0xD2, 0x04][..]));
        let remove = ida_y_vuelta_cliente(ClientMessage::VaultPassword { kind: vault_password::REMOVE_PIN, pin: 0, password: "test1".into() });
        assert_eq!(&remove[6..11], b"test1");
    }

    /// The other NPC dialogs OpenMU opens instead of `NpcWindowResponse` (C3 30).
    #[test]
    fn other_npc_dialogs() {
        // OpenNpcDialog (OpenMU ServerToClientPackets.xml:11779-11800, OpenNpcWindowPlugIn.cs:33-40):
        // C3 0C F9 01, the NPC number (543, Gens Duprian) little endian at 4 and the contribution at 8.
        let wire = [0xC3, 12, 0xF9, 0x01, 0x1F, 0x02, 0, 0, 0x10, 0x27, 0, 0];
        let msg = ServerMessage::OpenNpcDialog { npc: 543, contribution: 10_000 };
        assert_eq!(ServerMessage::decode(&wire), Some(msg.clone()));
        assert_eq!(msg.encode(), wire);
        // LegacyQuestStateDialog (C1 05 A1 <quest> <state>, ServerToClientPackets.xml:9029-9049).
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::QuestStateDialog { quest: 0, state: 0xFE }), [0xC1, 5, 0xA1, 0, 0xFE]);
        // ShowGuildMasterDialog: C1 03 54 (ServerToClientPackets.xml:7957-7967).
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::GuildMasterDialog), [0xC1, 3, 0x54]);
        // GuildMasterAnswer (ClientToServerPackets.xml): C1 04 54 <show creation dialog>.
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::GuildMasterAnswer { show_creation: false }), [0xC1, 4, 0x54, 0]);
    }

    /// The NPC dialogue window's requests (wsclientinline.h:826-948) and OpenMU's answers, captured
    /// from the live OpenMU talking to the Gens Duprian steward with a level 1 character.
    #[test]
    fn npc_dialogue_packets() {
        // Captured: C3 0C F9 01 1F 02 00 00 00 00 00 00 (543, no contribution).
        let open = [0xC3, 12, 0xF9, 0x01, 0x1F, 0x02, 0, 0, 0, 0, 0, 0];
        assert_eq!(ServerMessage::decode(&open), Some(ServerMessage::OpenNpcDialog { npc: 543, contribution: 0 }));

        assert_eq!(ida_y_vuelta_cliente(ClientMessage::AvailableQuestsRequest), [0xC1, 4, 0xF6, 0x30]);
        // Captured answer: no quests for the steward.
        let none = [0xC1, 8, 0xF6, 0x0A, 0x1F, 0x02, 0, 0];
        assert_eq!(ServerMessage::decode(&none), Some(ServerMessage::AvailableQuests { npc: 543, quests: vec![] }));
        assert_eq!(ServerMessage::AvailableQuests { npc: 543, quests: vec![] }.encode(), none);
        // Two quests (number in the low word, group in the high one), as OpenMU's
        // `QuestIdentification` (number at 0, group at 2).
        let two = ServerMessage::AvailableQuests { npc: 257, quests: vec![0x0012_0000, 0x0012_0003] };
        assert_eq!(
            ida_y_vuelta_servidor(two),
            [0xC1, 16, 0xF6, 0x0A, 0x01, 0x01, 2, 0, 0, 0, 0x12, 0, 3, 0, 0x12, 0]
        );
        // A count larger than the packet: only what it holds.
        let short = [0xC1, 12, 0xF6, 0x0A, 0x01, 0x01, 5, 0, 3, 0, 0x12, 0];
        assert_eq!(ServerMessage::decode(&short), Some(ServerMessage::AvailableQuests { npc: 257, quests: vec![0x0012_0003] }));
        // `SendQuestSelection`: DWORD then the line (OpenMU's QuestSelectRequest, 9 bytes).
        assert_eq!(
            ida_y_vuelta_cliente(ClientMessage::QuestSelect { quest: 0x0012_0003, selected: 2 }),
            [0xC1, 9, 0xF6, 0x0A, 3, 0, 0x12, 0, 2]
        );
        // QuestStepInfo (OpenMU: 11 bytes, step at 4 and group at 6).
        assert_eq!(
            ida_y_vuelta_servidor(ServerMessage::QuestStepInfo { quest: 0x0012_0001 }),
            [0xC1, 11, 0xF6, 0x0B, 1, 0, 0x12, 0, 0, 0, 0]
        );
        // QuestProgress (OpenMU: C1 FB F6 0C, number at 4, group at 6).
        let reward = ida_y_vuelta_servidor(ServerMessage::QuestRequestReward { quest: 0x0012_0001 });
        assert_eq!((reward.len(), &reward[..8]), (251, &[0xC1, 251, 0xF6, 0x0C, 1, 0, 0x12, 0][..]));
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::NpcBuffRequest), [0xC1, 4, 0xF6, 0x31]);
        // `SendQuestSelAnswer`: accept (1) the Spider Hunt (OpenMU's QuestProceedRequest).
        assert_eq!(
            ida_y_vuelta_cliente(ClientMessage::QuestProceed { quest: 0x0012_0000, answer: 1 }),
            [0xC1, 9, 0xF6, 0x0B, 0, 0, 0x12, 0, 1]
        );

        // Gens: the captured requests and answers.
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::GensJoin { gens: 1 }), [0xC1, 5, 0xF8, 0x01, 1]);
        // 3 = `GJEC_REG_GENS_LV_ERR` (level too low).
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::GensJoinResult { result: 3, gens: 1 }), [0xC1, 6, 0xF8, 0x02, 3, 1]);
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::GensReward { gens: 1 }), [0xC1, 5, 0xF8, 0x09, 1]);
        // 6 = `GENS_REWARD_NOT_REG`.
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::GensRewardResult { result: 6 }), [0xC1, 5, 0xF8, 0x0A, 6]);
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::GensLeave), [0xC1, 4, 0xF8, 0x03]);
        // 1 = `GSEC_IS_NOT_REG_GENS`.
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::GensLeaveResult { result: 1 }), [0xC1, 5, 0xF8, 0x04, 1]);
        // GensInfo (OpenMU ServerToClientPackets.xml: 24 bytes).
        let info = ServerMessage::GensInfo { gens: 2, ranking: 7, rank: 14, contribution: 500, next_contribution: 1500 };
        let wire = ida_y_vuelta_servidor(info);
        assert_eq!((wire.len(), &wire[..5], &wire[16..20]), (24, &[0xC1, 24, 0xF8, 0x07, 2][..], &500u32.to_le_bytes()[..]));
        // Legacy quest: `SendRequestQuestState` goes encrypted (C3).
        let set = ida_y_vuelta_cliente(ClientMessage::LegacyQuestState { quest: 0, state: 1 });
        assert_eq!(set, [0xC3, 5, 0xA2, 0, 1]);
    }

    /// OpenMU's `ObjectMessage` (C1 len 01 <id BE> <text with its 00>, ServerToClientPackets.xml).
    #[test]
    fn object_message() {
        let wire = ida_y_vuelta_servidor(ServerMessage::ObjectMessage { id: 0x020A, message: "Hi".into() });
        assert_eq!(wire, [0xC1, 8, 0x01, 0x02, 0x0A, b'H', b'i', 0]);
    }

    /// Dropping and picking up (wsclientinline.h:1122-1142) and OpenMU's answers
    /// (ServerToClientPackets.xml `ItemsDropped`, `MoneyDropped`, `ItemDropRemoved`,
    /// `ItemAddedToInventory`, `ItemDropResponse`, `ItemPickUpRequestFailed`).
    #[test]
    fn ground_item_packets() {
        // `SendRequestDropItem(MAX_EQUIPMENT_INDEX + 0, 130, 125)`: x, y, slot, encrypted.
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::DropItem { x: 130, y: 125, slot: 12 }), [0xC3, 6, 0x23, 130, 125, 12]);
        // `SendRequestGetItem(0x0123)`: the key in big endian, encrypted.
        assert_eq!(ida_y_vuelta_cliente(ClientMessage::PickUpItem { id: 0x0123 }), [0xC3, 5, 0x22, 0x01, 0x23]);

        // ItemDropResponse: success and slot.
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::ItemDropResult { success: true, slot: 12 }), [0xC1, 5, 0x23, 1, 12]);
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::ItemDropResult { success: false, slot: 0 }), [0xC1, 5, 0x23, 0, 0]);

        // ItemsDropped: one fresh Kris +0 (sword 1) at (130, 125), id 0x0012 with the 0x8000 flag.
        let kris = [0x01, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut wire = vec![0xC2, 0x00, 0x15, 0x20, 0x01, 0x80, 0x12, 130, 125];
        wire.extend_from_slice(&kris);
        let drop = DroppedItem { id: 0x12, fresh: true, x: 130, y: 125, item: kris };
        assert_eq!(ServerMessage::decode(&wire), Some(ServerMessage::ItemsDropped(vec![drop])));
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::ItemsDropped(vec![drop])), wire);
        assert_eq!(drop.zen(), None);

        // MoneyDropped (21 bytes): 1 234 567 zen = 0x12D687 at 10, 11 and 13; 15 at 9 and 0xE0 at 14.
        let money = [0xC2, 0x00, 0x15, 0x20, 0x01, 0x00, 0x34, 10, 20, 15, 0x12, 0xD6, 0x00, 0x87, 0xE0, 0, 0, 0, 0, 0, 0];
        let Some(ServerMessage::ItemsDropped(list)) = ServerMessage::decode(&money) else { panic!("money drop") };
        assert_eq!(list, [DroppedItem::money(0x34, false, 10, 20, 1_234_567)]);
        assert_eq!(list[0].zen(), Some(1_234_567));

        // ItemDropRemoved: two ids.
        let gone = [0xC2, 0x00, 0x09, 0x21, 0x02, 0x00, 0x12, 0x00, 0x34];
        assert_eq!(ServerMessage::decode(&gone), Some(ServerMessage::ItemDropRemoved(vec![0x12, 0x34])));
        assert_eq!(ServerMessage::ItemDropRemoved(vec![0x12, 0x34]).encode(), gone);

        // ItemAddedToInventory: slot and the item (16 bytes, encrypted).
        let mut added = vec![0xC3, 16, 0x22, 13];
        added.extend_from_slice(&kris);
        assert_eq!(ida_y_vuelta_servidor(ServerMessage::ItemPickedUp { slot: 13, item: kris }), added);
        // ItemPickUpRequestFailed: general, stacked and OpenMU's zen limit (4 bytes).
        for reason in [pick_up::FAILED, pick_up::STACKED, pick_up::ZEN] {
            assert_eq!(ida_y_vuelta_servidor(ServerMessage::ItemPickUpFailed { reason }), [0xC3, 4, 0x22, reason]);
        }
        // The 8-byte 0xFE is still the zen after picking it up.
        assert_eq!(
            ServerMessage::decode(&[0xC3, 0x08, 0x22, 0xFE, 0x00, 0x00, 0x30, 0x39]),
            Some(ServerMessage::InventoryMoneyUpdate { money: 12345 })
        );
    }
}
