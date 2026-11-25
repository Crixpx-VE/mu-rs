//! Talking to NPCs: the click, the walk next to the NPC, `SendRequestTalk` and the state of the
//! dialog the server opens (no windows yet: the shop window reads [`NpcDialog`]).
//!
//! **The click** (`MoveHero`, ZzzInterface.cpp:7528-7571): with an NPC under the mouse
//! (`SelectedNpc`) and neither the shop nor the vault open, the hero takes `MOVEMENT_TALK` with
//! `TargetNpc` and walks toward the NPC's tile with `PathFinding2`. That tile holds `TW_CHARACTER`
//! (`MoveCharactersClient`, ZzzCharacter.cpp:6317-6332), so the strict search fails and the loose
//! one ends on the reachable tile closest to it ([`mu_data::path::path_finding2`]). Without a path
//! (already next to it) it acts right away (`Action(c, o, true)`); otherwise on arriving
//! (`MovePath` ends → `Action(c, o, false)`, 7282-7295).
//!
//! **The action** (`Action` with `MOVEMENT_TALK`, ZzzInterface.cpp:3397-3586): `MouseUpdateTimeMax`
//! = 12, the hero stops (`SetPlayerStop`, `Movement = false`) and, for NPCs from number 205 on, the
//! rules of [`talk_rule`] decide: a message only (Chaos Goblin under level 10) or
//! `SendRequestTalk(key)` ([`ClientMessage::TalkToNpc`]). The hero does not turn toward the NPC.
//!
//! **The dialog** (`ReceiveTalk`, WSclient.cpp:5829-6001): `HideAll`, then the window of `Value`
//! (see [`WindowKind`]) and `SOUND_CLICK01` + `SOUND_INTERFACE01`. The NPC store or vault items
//! arrive after it (`ReceiveTradeInventory`, WSclient.cpp:1358-1415) and are kept in
//! [`NpcDialog::store`]. Some NPCs open their window with another packet: `ReceiveNPCDlgUIStart`
//! (C3 F9 01, the Gens NPCs), `ReceiveQuestState` (C1 A1, the quest NPCs) and
//! `ReceiveCreateGuildInterface` (C1 54, the guild master). Those windows do not exist yet, but they
//! count as an open dialog so that they close with their packet (OpenMU keeps the player in
//! `NpcDialogOpened` and ignores every other talk until then).
//!
//! **Closing** sends the packet of each window (`ClosingProcess`): `SendExitInventory` for the
//! shop, `SendRequestStorageExit` for the vault, `SendRequestMixExit` for the Chaos Machine. OpenMU
//! ignores a second talk while a dialog is open (`PlayerState.NpcDialogOpened`), so it must be sent.
//! It closes with Esc, when the hero walks (`SendMove` → `UpdateSendMoveInterface`), when the
//! inventory that goes with the shop, vault or Chaos Machine closes, and on map change (`HideAll`,
//! WSclient.cpp:1830). Logging out to the character selection sends `SendExitInventory` too
//! (`InitGame`, WSclient.cpp:653 from `ReceiveLogOut`, 745).

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use mu_protocol::game::{ClientMessage, ServerMessage, logout_kind};

use crate::chat_log::MsgType;
use crate::scene::GameScene;

pub struct NpcPlugin;

impl Plugin for NpcPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NpcDialog>()
            .init_resource::<TalkRequest>()
            .add_systems(PreUpdate, server_messages.in_set(NpcDialogSystems).after(crate::net::NetSystems))
            .add_systems(
                PreUpdate,
                esc_key.after(bevy::input::InputSystems).after(crate::chat::ChatKeys).run_if(in_state(GameScene::Game)),
            )
            .add_systems(
                Update,
                (talk_action.after(crate::character::HeroWalk), follow_inventory).run_if(in_state(GameScene::Game)),
            )
            .add_systems(OnExit(GameScene::Game), |mut d: ResMut<NpcDialog>, mut r: ResMut<TalkRequest>| {
                *d = NpcDialog::default();
                r.0 = None;
            });
    }
}

/// Where `NpcWindow` / `StoreItemList` are applied (in `PreUpdate`): a window that shows the dialog
/// (the shop) runs after it, so `HideAll` does not close what it opens.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct NpcDialogSystems;

/// `Action(c, o, …)` with `MOVEMENT_TALK` is due for the NPC with this scope key: set by the click
/// (no path) or on arriving, done by [`talk_action`].
#[derive(Resource, Default)]
pub struct TalkRequest(pub Option<u16>);

// --- Windows ----------------------------------------------------------------------------------------

/// What `ReceiveTalk` (WSclient.cpp:5836-5985) opens for each `Value`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    /// `INTERFACE_NPCSHOP`: `default` (0 merchant, 1...) and 0x22 (the gamble shop).
    Shop,
    /// 2: `INTERFACE_STORAGE`.
    Vault,
    /// 3 (Chaos Goblin), 0x13 (Jerridon), 0x15 (chaos card), 0x16 (cherry blossom):
    /// `INTERFACE_MIXINVENTORY`.
    ChaosMachine,
    /// 4: `INTERFACE_DEVILSQUARE`.
    DevilSquare,
    /// 6: `INTERFACE_BLOODCASTLE`.
    BloodCastle,
    /// 0x14: `INTERFACE_CURSEDTEMPLE_NPC`.
    CursedTemple,
    /// 0x21: `INTERFACE_DUELWATCH`.
    DuelWatch,
    /// 0x23: `INTERFACE_DOPPELGANGER_NPC`.
    Doppelganger,
    /// 0x24: `INTERFACE_EMPIREGUARDIAN_NPC`.
    EmpireGuardian,
    /// 0x25: `INTERFACE_UNITEDMARKETPLACE_NPC_JULIA`.
    Julia,
    /// `INTERFACE_NPC_DIALOGUE`, opened by `ReceiveNPCDlgUIStart` (C3 F9 01, WSclient.cpp:9881-9890)
    /// instead of `ReceiveTalk`: the Gens NPCs and others with talk lines (npc_dialogue.rs).
    NpcDialogue,
    /// `INTERFACE_QUEST_PROGRESS`, opened by `ReceiveQuestQSSelSentence` (C1 F6 0B, WSclient.cpp:
    /// 9750-9755) over the NPC dialogue, which it hides without its closing packet (`HideAllGroupA`
    /// uses `ShowInterface(..., false)`, NewUISystem.cpp:1345-1403); npc_dialogue.rs.
    QuestProgress,
    /// `INTERFACE_NPCQUEST`, opened by `ReceiveQuestState` (C1 A1, WSclient.cpp:9539-9546): Sevina,
    /// Marlon, Devin... No window yet.
    NpcQuest,
    /// `INTERFACE_NPCGUILDMASTER`, opened by `ReceiveCreateGuildInterface` (C1 54,
    /// WSclient.cpp:6700-6703). No window yet.
    GuildMaster,
    /// The rest with their own `case`: message boxes (7 `CTrainerMenuMsgBoxLayout`, 9
    /// `INDEX_NPC_LAHAP` `CGemIntegrationMsgBoxLayout`, 0x11 `CElpisMsgBoxLayout`, 0x12
    /// `COsbourneMsgBoxLayout`, 0x17/0x18 seed master/researcher menus, 0x19
    /// `CResetCharacterPointMsgBoxLayout`, 0x20 `CDelgardoMainMenuMsgBoxLayout`, 0x26
    /// `CLuckyTradeMenuMsgBoxLayout`, WSclient.cpp:5863-5977), the old server division window (5),
    /// `INTERFACE_SENATUS` (0x0C) and `SendRequestBCStatus` (0x0D). TODO: they need the message box
    /// framework (`CNewUIMessageBoxMng`), which does not exist yet.
    Other,
}

impl WindowKind {
    pub fn of(window: u8) -> Self {
        match window {
            2 => Self::Vault,
            3 | 0x13 | 0x15 | 0x16 => Self::ChaosMachine,
            4 => Self::DevilSquare,
            6 => Self::BloodCastle,
            0x14 => Self::CursedTemple,
            0x21 => Self::DuelWatch,
            0x22 => Self::Shop,
            0x23 => Self::Doppelganger,
            0x24 => Self::EmpireGuardian,
            0x25 => Self::Julia,
            // `INDEX_NPC_LAHAP` = 9 (_define.h:612); 0x26 with `LEM_ADD_LUCKYITEM` (active).
            5 | 7 | 9 | 0x0C | 0x0D | 0x11 | 0x12 | 0x17 | 0x18 | 0x19 | 0x20 | 0x26 => Self::Other,
            _ => Self::Shop,
        }
    }

    /// The packet its `OpenningProcess` sends: the Devil Square and Blood Castle entrances send
    /// `SendExitInventory` as soon as they open (NewUIEnterDevilSquare.cpp:274-276,
    /// NewUIBloodCastleEnter.cpp:278-280).
    fn open_packet(self) -> Option<ClientMessage> {
        matches!(self, Self::DevilSquare | Self::BloodCastle).then_some(ClientMessage::CloseNpc)
    }

    /// The packet its `ClosingProcess` sends.
    fn close_packet(self) -> Option<ClientMessage> {
        match self {
            // NewUINPCShop.cpp:430-432, NewUIEnterDevilSquare.cpp:315-317,
            // NewUIBloodCastleEnter.cpp:320-322, NewUIDuelWatchWindow.cpp:199-201.
            Self::Shop | Self::DevilSquare | Self::BloodCastle | Self::DuelWatch => Some(ClientMessage::CloseNpc),
            // `CNewUIStorageInventory::ProcessClosing` → `SendRequestStorageExit`.
            Self::Vault => Some(ClientMessage::CloseVault),
            // NewUIMixInventory.cpp:145-168: `MIXTYPE_GOBLIN_NORMAL`, `JERRIDON`, `CHAOS_CARD` and
            // `CHERRYBLOSSOM` (the types `ReceiveTalk` sets) → `SendRequestMixExit`.
            Self::ChaosMachine => Some(ClientMessage::CloseCrafting),
            // `CNewUINPCDialogue::ProcessClosing` (NewUINPCDialogue.cpp:379-386) and
            // `CNewUINPCQuest::ProcessClosing` (NewUINPCQuest.cpp:463-467): `SendExitInventory`.
            Self::NpcDialogue | Self::NpcQuest => Some(ClientMessage::CloseNpc),
            // `CNewUIQuestProgress::ProcessClosing` (NewUIQuestProgress.cpp:347-353).
            Self::QuestProgress => Some(ClientMessage::CloseNpc),
            // `CNewUIGuildMakeWindow::ClosingProcess` (NewUIGuildMakeWindow.cpp:247-253):
            // `SendRequestGuildMaster(FALSE)`, which OpenMU also takes as closing the dialog
            // (`GuildMasterAnswerAction`).
            Self::GuildMaster => Some(ClientMessage::GuildMasterAnswer { show_creation: false }),
            // Commented out in NewUIDoppelGangerWindow.cpp:212-215 and NewUIUnitedMarketPlaceWindow.cpp:
            // 227-230; empty in NewUIEmpireGuardianNPC.cpp.
            Self::Doppelganger | Self::Julia | Self::EmpireGuardian => None,
            // TODO: each of these closes in its own way (message boxes, NewUICursedTempleEnter.cpp,
            // `CNewUICastleWindow::ClosingProcess`); not checked yet, so nothing is sent.
            Self::CursedTemple | Self::Other => None,
        }
    }

    /// `ShowInterface(INTERFACE_INVENTORY)` goes with it (NewUISystem.cpp:652-671) and hiding one
    /// hides the other (985-1022 and 1051-1085).
    fn with_inventory(self) -> bool {
        matches!(self, Self::Shop | Self::Vault | Self::ChaosMachine)
    }

    /// Hidden by `UpdateSendMoveInterface` (NewUISystem.cpp:1662-1753) when the hero walks; the guild
    /// master, quest and dialogue windows at 1673-1684 and 1725-1728.
    fn closes_on_move(self) -> bool {
        matches!(
            self,
            Self::Shop
                | Self::Vault
                | Self::DevilSquare
                | Self::BloodCastle
                | Self::CursedTemple
                | Self::DuelWatch
                | Self::Doppelganger
                | Self::EmpireGuardian
                | Self::Julia
                | Self::NpcDialogue
                | Self::QuestProgress
                | Self::NpcQuest
                | Self::GuildMaster
        )
    }

    /// Closed with Esc by its own `UpdateKeyEvent` (NewUINPCShop.cpp:189-197,
    /// NewUIStorageInventory.cpp:123, the inventory's for the Chaos Machine (NewUIMyInventory.cpp:564),
    /// NewUIEnterDevilSquare.cpp:129, NewUIBloodCastleEnter.cpp:144, NewUICursedTempleEnter.cpp:227,
    /// NewUIDuelWatchWindow.cpp:97, NewUIDoppelGangerWindow.cpp:105, NewUIEmpireGuardianNPC.cpp:91,
    /// NewUIUnitedMarketPlaceWindow.cpp:108, NewUINPCDialogue.cpp:211-213, NewUINPCQuest.cpp:155-157,
    /// NewUIGuildMakeWindow.cpp:445-447). The message boxes of `Other`: not yet.
    fn closes_on_esc(self) -> bool {
        self != Self::Other
    }
}

/// The NPC dialog: who was talked to and what the server opened. For the shop window.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct NpcDialog {
    /// Scope key (`CharactersClient[TargetNpc].Key`) of the last NPC talked to.
    pub npc_key: Option<u16>,
    /// Its monster number (`MonsterIndex`).
    pub npc_number: Option<u16>,
    /// `g_pNPCShop->SetRepairShop` (ZzzInterface.cpp:3424-3438): it repairs.
    pub repair: bool,
    /// `ReceiveTalk`'s `Value` of the open window (see [`WindowKind::of`]); `None` when closed or
    /// when the window came from another packet (see [`NpcDialog::open_kind`]).
    pub window: Option<u8>,
    /// The open window; `None` when closed.
    kind: Option<WindowKind>,
    /// Counts the windows opened, so a window that replaces another of the same kind is seen as new.
    pub serial: u32,
    /// Items of the NPC store or vault (`ReceiveTradeInventory`): slot and the 12 item bytes.
    pub store: Vec<(u8, [u8; 12])>,
    /// The inventory was open the last time it was looked at (to see it close).
    inventory_seen: bool,
}

impl NpcDialog {
    pub fn kind(&self) -> Option<WindowKind> {
        self.kind
    }

    #[allow(dead_code)] // Used by the tests.
    pub fn is_open(&self) -> bool {
        self.kind.is_some()
    }

    /// `SendRequestTalk` went out for this NPC.
    fn talked(&mut self, key: u16, number: u16, repair: bool) {
        self.npc_key = Some(key);
        self.npc_number = Some(number);
        self.repair = repair;
    }

    /// `ReceiveTalk`: `HideAll` closes the open window (with its packet), then the new one opens.
    /// Returns the packets to send.
    fn open(&mut self, window: u8) -> Vec<ClientMessage> {
        let mut out = self.open_kind(WindowKind::of(window));
        self.window = Some(window);
        out.extend(WindowKind::of(window).open_packet());
        out
    }

    /// A dialog window opened by a packet other than `ReceiveTalk` (`OpenNpcDialog`, the quest
    /// state, the guild master): like `open`, the window open before closes with its packet (the
    /// server keeps one NPC dialog at a time, OpenMU `TalkNpcAction.cs:38-50`).
    fn open_kind(&mut self, kind: WindowKind) -> Vec<ClientMessage> {
        let out: Vec<ClientMessage> = self.close().into_iter().collect();
        self.kind = Some(kind);
        self.serial = self.serial.wrapping_add(1);
        out
    }

    /// `Show` of a window that hides the open one with `HideAllGroupA`'s `ShowInterface(..., false)`:
    /// no `ProcessClosing`, so no closing packet (the server's dialog goes on).
    pub(crate) fn replace_kind(&mut self, kind: WindowKind) {
        self.window = None;
        self.store.clear();
        self.kind = Some(kind);
        self.serial = self.serial.wrapping_add(1);
    }

    /// `ReceiveTradeInventory`: `SubCode` 3 and 5 are Chaos Machine results (not here yet); the rest
    /// empty the store and fill it if the shop or the vault is open. Returns whether it was kept.
    fn set_store(&mut self, sub_code: u8, items: &[(u8, [u8; 12])]) -> bool {
        if matches!(sub_code, 3 | 5) {
            return false;
        }
        self.store.clear();
        if matches!(self.kind(), Some(WindowKind::Shop | WindowKind::Vault)) {
            self.store = items.to_vec();
            return true;
        }
        false
    }

    /// `Hide` of the window: its `ClosingProcess` packet. The talked-to NPC is kept.
    fn close(&mut self) -> Option<ClientMessage> {
        let kind = self.kind.take()?;
        self.window = None;
        self.store.clear();
        kind.close_packet()
    }

    /// `IsImpossibleSendMoveInterface` (NewUISystem.cpp:1568-1581): with the Chaos Machine open the
    /// hero does not walk.
    pub fn blocks_move(&self) -> bool {
        self.kind() == Some(WindowKind::ChaosMachine)
    }

    /// With the shop or the vault open a click on an NPC is a click on the ground
    /// (ZzzInterface.cpp:7528-7531).
    pub fn blocks_talk(&self) -> bool {
        matches!(self.kind(), Some(WindowKind::Shop | WindowKind::Vault))
    }
}

/// Closes the dialog: its packet and, for the windows that come with it, the inventory.
pub(crate) fn close_dialog(dialog: &mut NpcDialog, net: &mut crate::net::Net, inv: &mut crate::inventory::Inventory, why: &str) {
    close_dialog_with(dialog, net, inv, why, None);
}

/// [`close_dialog`], sending `fallback` when the window has no closing packet of its own.
fn close_dialog_with(
    dialog: &mut NpcDialog,
    net: &mut crate::net::Net,
    inv: &mut crate::inventory::Inventory,
    why: &str,
    fallback: Option<ClientMessage>,
) {
    let Some(kind) = dialog.kind() else { return };
    let packet = dialog.close().or(fallback);
    info!("NPC: se cierra la ventana {kind:?} ({why}), se manda {packet:?}");
    if let Some(p) = packet
        && net.connected()
    {
        net.send(p);
    }
    if kind.with_inventory() && inv.open {
        inv.toggle();
    }
    dialog.inventory_seen = inv.open;
}

// --- The click --------------------------------------------------------------------------------------

/// What the click on an NPC needs, for `hero_input` (character.rs).
#[derive(SystemParam)]
pub struct NpcTalk<'w, 's> {
    pub dialog: ResMut<'w, NpcDialog>,
    request: ResMut<'w, TalkRequest>,
    hovered: Res<'w, crate::names::Hovered>,
    inventory: ResMut<'w, crate::inventory::Inventory>,
    monsters: Query<'w, 's, &'static crate::monsters::Monster>,
    characters: Query<'w, 's, (&'static crate::names::Selectable, &'static Transform, Option<&'static crate::monsters::Monster>)>,
}

impl NpcTalk<'_, '_> {
    /// `SelectedNpc` if a click may talk to it: its key and tile (`Position / TERRAIN_SCALE`).
    pub fn clicked_npc(&self) -> Option<(u16, (usize, usize))> {
        if self.dialog.blocks_talk() {
            return None;
        }
        let m = self.monsters.get(self.hovered.npc?).ok()?;
        Some((m.key, (m.pos.x.max(0.0) as usize, m.pos.y.max(0.0) as usize)))
    }

    /// `PathFinding2` from `from` to the NPC's tile, with `TW_CHARACTER` on every live character's
    /// tile except traps (`MoveCharactersClient`, ZzzCharacter.cpp:6317-6332).
    pub fn path_to(&self, terrain: &mu_data::terrain::Terrain, from: (usize, usize), to: (usize, usize)) -> Option<Vec<(usize, usize)>> {
        use mu_data::terrain::attr;
        let occupied: std::collections::HashSet<(usize, usize)> = self
            .characters
            .iter()
            .filter(|(s, _, m)| !s.dead && !m.is_some_and(|m| m.def.kind() == mu_data::monsters::Kind::Trap))
            .map(|(_, tf, _)| (tf.translation.x.max(0.0) as usize, (-tf.translation.z).max(0.0) as usize))
            .collect();
        let wall = |x: usize, y: usize| {
            let a = terrain.attribute(x, y) & !attr::CHARACTER;
            if occupied.contains(&(x, y)) { a | attr::CHARACTER } else { a }
        };
        mu_data::path::path_finding2(from, to, wall)
    }

    /// `Action(c, o, true)` now.
    pub fn act(&mut self, key: u16) {
        self.request.0 = Some(key);
    }

    /// `UpdateSendMoveInterface` (NewUISystem.cpp:1662-1753): the NPC windows close.
    pub fn after_send_move(&mut self, net: &mut crate::net::Net) {
        if self.dialog.kind().is_some_and(WindowKind::closes_on_move) {
            close_dialog(&mut self.dialog, net, &mut self.inventory, "el personaje camina");
        }
    }
}

// --- The action -------------------------------------------------------------------------------------

/// What `Action` with `MOVEMENT_TALK` does for an NPC.
#[derive(Debug, Clone, PartialEq)]
pub enum TalkOutcome {
    /// Nothing (NPCs under number 205).
    Nothing,
    /// Only a system message.
    Message(String),
    /// `SendRequestTalk`, after `SetRepairShop(repair)`, and then maybe a system message.
    Send { repair: bool, message: Option<String> },
    /// The original does something else here that is not implemented yet.
    NotImplemented(&'static str),
}

/// `CHAOS_MIX_LEVEL` (_define.h, MuMain _define.h:283).
const CHAOS_MIX_LEVEL: u32 = 10;

/// `GlobalText[663]` with `CHAOS_MIX_LEVEL` (English text from MuMain Localization/Game.en.resx:2229).
fn chaos_level_text() -> String {
    format!("Only level above {CHAOS_MIX_LEVEL} can do the Chaos Combination.")
}

/// The rules of `Action` with `MOVEMENT_TALK` (ZzzInterface.cpp:3408-3583) for NPC `number`, the hero's
/// `level` and the map folder `world` (`WorldActive` + 1).
pub fn talk_rule(number: u16, level: u32, world: u16) -> TalkOutcome {
    // `npcIndex` = 205 (3410-3414): below it nothing happens.
    if number < 205 {
        return TalkOutcome::Nothing;
    }
    // The Chaos Goblin (3417-3423).
    if number == 238 && level < CHAOS_MIX_LEVEL {
        return TalkOutcome::Message(chaos_level_text());
    }
    // Repair shops (3424-3438): 578 with `ASG_ADD_KARUTAN_NPC` (active).
    let repair = matches!(number, 243 | 246 | 251 | 416 | 578);
    let send = |message: Option<String>| TalkOutcome::Send { repair, message };
    // TODO: `INTERFACE_MYQUEST` is hidden (3440-3443) and `SendRequestQuestHistory` sent when the
    // quest data is loaded (3445-3448); 229 sets `bCheckNPC` (3564-3567) and 226 asks for the dark
    // horse / dark raven info (3569-3582). None of that exists yet.
    match world as i32 - 1 {
        // `WD_34CRYWOLF_1ST` (3450-3487): altars and the Werewolf Quarel message box depend on the
        // Crywolf state.
        34 => return TalkOutcome::NotImplemented("Crywolf (ZzzInterface.cpp:3450-3487)"),
        // `IsBalgasBarrackMap` (`WD_41CHANGEUP3RD_1ST`, 3488-3493): talk, then a message box (TODO).
        41 => return send(None),
        _ => {}
    }
    match number {
        // 3494-3509: the Santa Village NPCs; 470 and 471 also say `GlobalText[2596]` / `[2597]` with
        // 100 (English text from MuMain Monsters.json, "Little Santa" and "Cupid").
        470 => send(Some("Health has been recovered of 100%.".into())),
        471 => send(Some("Mana has been recovered of 100%.".into())),
        468..=475 | 478 | 540 | 547 | 579 => send(None),
        _ => match world as i32 - 1 {
            // `Is_Kanturu2nd` (3528-3534): talk unless the entrance NPC is animating (not tracked yet).
            38 => send(None),
            // `IsCursedTemple` (3535-3555): `CheckTalkProgressNpc` instead of `SendRequestTalk`.
            45..=50 => TalkOutcome::NotImplemented("Cursed Temple (ZzzInterface.cpp:3535-3555)"),
            _ => send(None),
        },
    }
}

/// `Action` with `MOVEMENT_TALK` (ZzzInterface.cpp:3397-3586).
#[allow(clippy::too_many_arguments)]
fn talk_action(
    mut request: ResMut<TalkRequest>,
    mut timer: ResMut<crate::character::MouseTimer>,
    mut players: Query<&mut crate::character::Player>,
    monsters: Query<&crate::monsters::Monster>,
    args: Res<crate::Args>,
    hero: Option<Res<crate::account::Hero>>,
    mut dialog: ResMut<NpcDialog>,
    (mut chat, metrics): (ResMut<crate::chat::Chat>, Res<crate::chat::TextMetrics>),
    mut net: ResMut<crate::net::Net>,
    mut inv: ResMut<crate::inventory::Inventory>,
) {
    let Some(key) = request.0.take() else { return };
    timer.max = 12;
    // `SetPlayerStop(c); c->Movement = false` (except for an elf in the Crywolf elf-only state, 3400-3406:
    // that state does not exist yet).
    if let Ok(mut p) = players.single_mut() {
        p.path.clear();
    }
    // `TargetNpc` points at a character that is no longer there: nothing to talk to.
    let Some(npc) = monsters.iter().find(|m| m.key == key) else { return };
    let level = hero.as_ref().map_or(0, |h| h.level);
    let mut system = |text: &str| {
        chat.add("", text, MsgType::System, MsgType::All, &metrics);
    };
    match talk_rule(npc.number, level, args.world) {
        TalkOutcome::Nothing => {}
        TalkOutcome::Message(text) => system(&text),
        TalkOutcome::Send { repair, message } => {
            info!("NPC {} (número {}): se le habla", key, npc.number);
            // Safety net (not in the original): OpenMU ignores a talk while its player is still in
            // `NpcDialogOpened` (`TalkNpcAction.cs:38-50`), so a dialog this client thinks is open is
            // closed first, with `SendExitInventory` if the window has no packet of its own.
            close_dialog_with(&mut dialog, &mut net, &mut inv, "se habla con otro NPC", Some(ClientMessage::CloseNpc));
            dialog.talked(key, npc.number, repair);
            if net.connected() {
                net.send(ClientMessage::TalkToNpc { id: key });
            }
            if let Some(text) = message {
                system(&text);
            }
        }
        TalkOutcome::NotImplemented(what) => warn!("NPC {} (número {}): sin implementar: {what}", key, npc.number),
    }
}

// --- The dialog -------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn server_messages(
    mut from: MessageReader<crate::net::FromServer>,
    mut dialog: ResMut<NpcDialog>,
    mut net: ResMut<crate::net::Net>,
    mut inv: ResMut<crate::inventory::Inventory>,
    (mut game_ui, mut char_info, mut move_window): (
        ResMut<crate::options::GameUi>,
        ResMut<crate::char_info::CharInfoWindow>,
        ResMut<crate::move_window::MoveWindow>,
    ),
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut players: Query<&mut crate::character::Player>,
) {
    for m in from.read() {
        let send = |packets: Vec<ClientMessage>, net: &mut crate::net::Net| {
            for p in packets {
                if net.connected() {
                    net.send(p);
                }
            }
        };
        // `g_pNewUISystem->HideAll()` of the windows that exist here (the NPC dialog closes in
        // `open`/`open_kind` with its packet).
        let mut hide_all = |inv: &mut crate::inventory::Inventory, dialog: &mut NpcDialog| {
            if inv.open {
                inv.toggle();
            }
            char_info.close();
            move_window.close();
            // (`hide_all` would also close the system menu box, a message box `HideAll` leaves.)
            if !game_ui.modal() {
                game_ui.hide_all();
            }
            dialog.inventory_seen = false;
        };
        match &m.0 {
            ServerMessage::NpcWindow { window } => {
                // `HideAll` (WSclient.cpp:5834), then `Show` of the window.
                send(dialog.open(*window), &mut net);
                hide_all(&mut inv, &mut dialog);
                sound.play("SOUND_CLICK01");
                sound.play("SOUND_INTERFACE01");
                // (Outside window mode the original also moves the mouse to the left of the screen,
                // `SetCursorPos`, 5988-5998: the game runs in a window here.)
                info!(
                    "NPC {:?} (número {:?}): se abre la ventana {window} ({:?}){}",
                    dialog.npc_key,
                    dialog.npc_number,
                    WindowKind::of(*window),
                    if dialog.repair { ", repara" } else { "" }
                );
            }
            ServerMessage::OpenNpcDialog { npc, contribution } => {
                // `ReceiveNPCDlgUIStart` (WSclient.cpp:9881-9890): `Show(INTERFACE_NPC_DIALOGUE)` unless
                // it is already open (the window is npc_dialogue.rs').
                if dialog.kind() != Some(WindowKind::NpcDialogue) {
                    send(dialog.open_kind(WindowKind::NpcDialogue), &mut net);
                }
                info!("NPC {npc}: se abre su diálogo (contribución gens {contribution})");
            }
            ServerMessage::QuestStateDialog { quest, state } => {
                // `ReceiveQuestState` (WSclient.cpp:9539-9546): `HideAll`, then `Show(INTERFACE_NPCQUEST)`.
                // The quest window is not implemented yet.
                send(dialog.open_kind(WindowKind::NpcQuest), &mut net);
                hide_all(&mut inv, &mut dialog);
                info!("NPC: ventana de misión {quest} (estado {state:#04x}; la ventana aún no existe)");
            }
            ServerMessage::GuildMasterDialog => {
                // `ReceiveCreateGuildInterface` (WSclient.cpp:6700-6703): `Show(INTERFACE_NPCGUILDMASTER)`.
                // The guild creation window is not implemented yet.
                send(dialog.open_kind(WindowKind::GuildMaster), &mut net);
                info!("NPC: ventana del maestro de gremios (aún no existe)");
            }
            ServerMessage::StoreItemList { kind, items } => {
                let kept = dialog.set_store(*kind, items);
                info!("NPC: lista de la tienda (tipo {kind}): {} objetos{}", items.len(), if kept { "" } else { " (ignorada)" });
            }
            ServerMessage::MapChanged { is_map_change, .. } => {
                // `ReceiveTeleport`: `Hero->Movement = false` ends any walk to an NPC; on a map change
                // `HideAll` (WSclient.cpp:1830) closes the dialog.
                for mut p in &mut players {
                    p.talk = None;
                }
                if *is_map_change {
                    close_dialog(&mut dialog, &mut net, &mut inv, "cambio de mapa");
                }
            }
            ServerMessage::LogoutResponse { kind } if *kind == logout_kind::BACK_TO_CHARACTER_SELECTION => {
                // `ReceiveLogOut` → `InitGame` → `SendExitInventory` (WSclient.cpp:745 and 653),
                // whatever was open.
                *dialog = NpcDialog::default();
                if net.connected() {
                    net.send(ClientMessage::CloseNpc);
                }
            }
            _ => {}
        }
    }
}

/// Esc closes the dialog before the system menu sees it (like the inventory).
#[allow(clippy::too_many_arguments)]
fn esc_key(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    game_ui: Res<crate::options::GameUi>,
    move_window: Res<crate::move_window::MoveWindow>,
    mut dialog: ResMut<NpcDialog>,
    mut net: ResMut<crate::net::Net>,
    mut inv: ResMut<crate::inventory::Inventory>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    shop: Res<crate::shop::NpcShop>,
) {
    if !keys.just_pressed(KeyCode::Escape)
        // Not while a sale waits for the server (NewUINPCShop.cpp:191).
        || shop.selling
        || game_ui.modal()
        || game_ui.option_open()
        || game_ui.window_menu_open()
        || move_window.is_open()
        || !dialog.kind().is_some_and(WindowKind::closes_on_esc)
    {
        return;
    }
    // The NPC dialogue and quest progress windows play it in their `ProcessClosing` (npc_dialogue.rs).
    if !matches!(dialog.kind(), Some(WindowKind::NpcDialogue | WindowKind::QuestProgress)) {
        sound.play("SOUND_CLICK01");
    }
    close_dialog(&mut dialog, &mut net, &mut inv, "Esc");
    keys.clear_just_pressed(KeyCode::Escape);
}

/// Hiding the inventory hides the shop, the vault and the Chaos Machine (NewUISystem.cpp:985-1022).
fn follow_inventory(mut dialog: ResMut<NpcDialog>, mut net: ResMut<crate::net::Net>, mut inv: ResMut<crate::inventory::Inventory>) {
    let was_open = std::mem::replace(&mut dialog.inventory_seen, inv.open);
    if was_open && !inv.open && dialog.kind().is_some_and(WindowKind::with_inventory) {
        close_dialog(&mut dialog, &mut net, &mut inv, "se cierra el inventario");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ventanas() {
        assert_eq!(WindowKind::of(0), WindowKind::Shop);
        assert_eq!(WindowKind::of(1), WindowKind::Shop);
        assert_eq!(WindowKind::of(0x1A), WindowKind::Shop, "no case of its own: the shop");
        assert_eq!(WindowKind::of(0x22), WindowKind::Shop);
        assert_eq!(WindowKind::of(2), WindowKind::Vault);
        for w in [3, 0x13, 0x15, 0x16] {
            assert_eq!(WindowKind::of(w), WindowKind::ChaosMachine);
        }
        assert_eq!(WindowKind::of(9), WindowKind::Other);
    }

    #[test]
    fn abrir_y_cerrar() {
        let mut d = NpcDialog::default();
        d.talked(0x1234, 251, true);
        assert!(d.open(0).is_empty());
        assert!(d.is_open() && d.blocks_talk() && !d.blocks_move());
        let kris = [1; 12];
        assert!(d.set_store(0, &[(16, kris)]));
        assert_eq!(d.store, vec![(16, kris)]);
        // Chaos Machine results do not touch the store.
        assert!(!d.set_store(3, &[]));
        assert_eq!(d.store.len(), 1);
        assert_eq!(d.close(), Some(ClientMessage::CloseNpc));
        assert!(!d.is_open() && d.store.is_empty());
        assert_eq!(d.npc_key, Some(0x1234), "the NPC talked to is kept");
        assert_eq!(d.close(), None, "already closed");
    }

    #[test]
    fn paquetes_de_cada_ventana() {
        let mut d = NpcDialog::default();
        d.open(2);
        assert!(d.blocks_talk());
        // `HideAll` closes the vault before the Chaos Machine opens.
        assert_eq!(d.open(3), vec![ClientMessage::CloseVault]);
        assert!(d.blocks_move() && !d.blocks_talk());
        assert!(!d.set_store(0, &[(0, [0; 12])]), "the store list only goes to the shop or vault");
        assert_eq!(d.close(), Some(ClientMessage::CloseCrafting));
        // Devil Square: `SendExitInventory` on opening and on closing.
        assert_eq!(d.open(4), vec![ClientMessage::CloseNpc]);
        assert_eq!(d.close(), Some(ClientMessage::CloseNpc));
        assert!(d.open(0x23).is_empty());
        assert_eq!(d.close(), None);
    }

    /// The dialogs OpenMU opens without `NpcWindowResponse` (Gens NPCs, quest NPCs, guild master) must
    /// count as open, or the server would ignore every later talk (it keeps `NpcDialogOpened`).
    #[test]
    fn dialogs_without_receive_talk() {
        let mut d = NpcDialog::default();
        assert!(d.open_kind(WindowKind::NpcDialogue).is_empty());
        assert!(d.is_open() && d.window.is_none() && !d.blocks_talk());
        assert!(WindowKind::NpcDialogue.closes_on_move() && WindowKind::NpcDialogue.closes_on_esc());
        // The quest window replaces it, closing it first.
        assert_eq!(d.open_kind(WindowKind::NpcQuest), vec![ClientMessage::CloseNpc]);
        assert_eq!(d.kind(), Some(WindowKind::NpcQuest));
        // A shop after it: `HideAll` closes the quest window with `SendExitInventory`.
        assert_eq!(d.open(0), vec![ClientMessage::CloseNpc]);
        assert_eq!(d.kind(), Some(WindowKind::Shop));
        d.close();
        d.open_kind(WindowKind::GuildMaster);
        assert_eq!(d.close(), Some(ClientMessage::GuildMasterAnswer { show_creation: false }));
        assert!(!d.is_open());
    }

    /// The quest progress window hides the NPC dialogue without its `SendExitInventory` and sends it
    /// itself when it closes.
    #[test]
    fn quest_progress_replaces_the_dialogue() {
        let mut d = NpcDialog::default();
        d.open_kind(WindowKind::NpcDialogue);
        let serial = d.serial;
        d.replace_kind(WindowKind::QuestProgress);
        assert_eq!((d.kind(), d.serial), (Some(WindowKind::QuestProgress), serial + 1));
        assert!(WindowKind::QuestProgress.closes_on_move() && WindowKind::QuestProgress.closes_on_esc());
        assert_eq!(d.close(), Some(ClientMessage::CloseNpc));
    }

    #[test]
    fn que_cierra_cada_cosa() {
        assert!(WindowKind::Shop.closes_on_move() && WindowKind::Vault.closes_on_move());
        assert!(!WindowKind::ChaosMachine.closes_on_move(), "the hero cannot walk with it open");
        assert!(!WindowKind::Other.closes_on_move() && !WindowKind::Other.closes_on_esc());
        assert!(WindowKind::ChaosMachine.with_inventory() && !WindowKind::DevilSquare.with_inventory());
    }

    #[test]
    fn reglas_al_hablar() {
        let lorencia = 1;
        assert_eq!(talk_rule(200, 50, lorencia), TalkOutcome::Nothing, "under 205");
        assert_eq!(talk_rule(238, 9, 3), TalkOutcome::Message(chaos_level_text()));
        assert_eq!(talk_rule(238, 10, 3), TalkOutcome::Send { repair: false, message: None });
        assert_eq!(talk_rule(251, 1, lorencia), TalkOutcome::Send { repair: true, message: None });
        assert_eq!(talk_rule(240, 1, lorencia), TalkOutcome::Send { repair: false, message: None });
        assert!(matches!(talk_rule(470, 1, 63), TalkOutcome::Send { message: Some(_), .. }));
        assert!(matches!(talk_rule(240, 1, 35), TalkOutcome::NotImplemented(_)), "Crywolf");
        assert!(matches!(talk_rule(380, 1, 46), TalkOutcome::NotImplemented(_)), "Cursed Temple");
        assert_eq!(talk_rule(578, 1, 81), TalkOutcome::Send { repair: true, message: None });
    }
}
