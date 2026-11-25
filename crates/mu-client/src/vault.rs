//! The vault (warehouse): `SEASON3B::CNewUIStorageInventory` (NewUIStorageInventory.cpp) with its
//! 8×15 `CNewUIInventoryCtrl` grid, next to the inventory.
//!
//! - **Opening and closing**: it opens when npc.rs has the vault dialog (`ReceiveTalk` 2 →
//!   `Show(INTERFACE_STORAGE)`, which also shows the inventory, NewUISystem.cpp:666-671) and fills its
//!   grid from the item list that follows (`ReceiveTradeInventory` → `InsertItem`, WSclient.cpp:
//!   1405-1408). OpenMU then sends the vault's zen (`ReceiveStorageGold`, C1 81) and its lock state
//!   (`ReceiveStorageStatus`, C1 83). Closing (its top X, Esc, walking, closing the inventory...) goes
//!   through npc.rs, which sends `SendRequestStorageExit` (C1 82); not while a move waits for its
//!   answer (`ProcessClosing`, NewUIStorageInventory.cpp:273-282).
//! - **Drawing** at (260, 0) (`Create(m_pNewUIMng, 260, 0)`, NewUISystem.cpp:203-204): the frame, the
//!   title with the lock state, the vault's zen and the storage fee (`RenderBackImage`,
//!   `RenderText`), the grid and its items in 3D (item_view.rs) with their tooltips, and the deposit,
//!   withdraw and lock buttons. It draws inside the inventory's layers (inventory.rs), like the shop.
//! - **Moving items** (`ProcessInventoryCtrl`, NewUIStorageInventory.cpp:298-347): an item is picked
//!   up from the vault grid like from the inventory's; the held item pressed over a free place of the
//!   vault asks the server to move it there (C3 24 with the vault storage; `SendRequestItemToStorage`
//!   refuses the items of `IsStoreBan`), and one picked up from the vault dropped on the inventory
//!   asks to move it back (inventory.rs). The item stays on the mouse until the server answers
//!   (`ItemMoved` / `ItemMoveFailed`, applied by inventory.rs). A right click moves an item to the
//!   first free place of the other window (`ProcessStorageItemAutoMove`, `ProcessMyInvenItemAutoMove`),
//!   which it leaves when the server says where it went.
//! - **Zen**: the deposit and withdraw buttons open the original's number boxes (`CZenReceiptMsgBoxLayout`
//!   and `CZenPaymentMsgBoxLayout`, NewUICustomMessageBox.cpp:4481-4627: a `CNewUITextInputMsgBox`);
//!   their OK sends `SendRequestStorageGold` (C1 81) and both zen amounts change when the server
//!   answers.
//!
//! The server is the only authority over items and zen: nothing here changes them by itself.
//!
//! Not yet: the vault lock. OpenMU supports it (`UnlockVault`, `SetVaultPin`, `RemoveVaultPin`,
//! [`ClientMessage::VaultPassword`]), but the original's keypad boxes (`CPasswordKeyPadMsgBoxLayout`,
//! `CStorageLockKeyPadMsgBoxLayout`, `CStorageUnlockMsgBoxLayout`) are not built: the lock button does
//! nothing and, with a locked vault, items and zen cannot be taken out (what the keypad's Cancel
//! leaves). The in-game shop's error box after a refused item (`CMsgBoxIGSCommon`,
//! NewUIStorageInventory.cpp:430-435) is not built either: only the chat message is shown.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use mu_data::items::info::item_info;
use mu_data::items::{Item, group, price};
use mu_protocol::game::{ClientMessage, ServerMessage, storage, vault_state};

use crate::chat_log::MsgType;
use crate::inventory::{
    self, BTN_H, BTN_W, CtrlEvent, DROP_NORMAL, GridItem, Inventory, InventorySet, Picked, Piece, SQUARE, Source, Tex, VAULT_COLUMNS, VAULT_ROWS,
    inside,
};
use crate::item_view::{ItemData, ItemView};
use crate::npc::{NpcDialog, NpcDialogSystems, WindowKind};
use crate::scene::GameScene;
use crate::sound::SoundQueue;

pub struct VaultPlugin;

impl Plugin for VaultPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Vault>()
            .init_resource::<VaultDraw>()
            .add_systems(PreUpdate, (server_messages, follow_dialog).chain().after(NpcDialogSystems).after(crate::net::NetSystems))
            .add_systems(
                PreUpdate,
                keys.after(bevy::input::InputSystems).before(crate::chat::ChatKeys).run_if(in_state(GameScene::Game)),
            )
            .add_systems(
                Update,
                (update_mouse, prepare_draw)
                    .chain()
                    .after(InventorySet::Mouse)
                    .before(InventorySet::Draw)
                    .run_if(in_state(GameScene::Game)),
            )
            .add_systems(OnExit(GameScene::Game), |mut vault: ResMut<Vault>, mut draw: ResMut<VaultDraw>, mut ui: ResMut<crate::options::GameUi>| {
                *vault = Vault::default();
                *draw = VaultDraw::default();
                ui.msg_box = false;
            });
    }
}

// --- Measurements (640×480 screen) -------------------------------------------------------------

/// `Create(m_pNewUIMng, 260, 0)` (NewUISystem.cpp:204), `STORAGE_WIDTH` × `STORAGE_HEIGHT`
/// (NewUIStorageInventory.h:39-43).
const POS: (f32, f32) = (260.0, 0.0);
const W: f32 = 190.0;
const H: f32 = 429.0;
/// The grid: `Create(..., x + 15, y + 36, 8, 15)` (NewUIStorageInventory.cpp:42-43).
const GRID: (f32, f32) = (POS.0 + 15.0, POS.1 + 36.0);
/// The buttons, 36×29 at `x + {38, 78, 118}`, `y + 390` (NewUIStorageInventory.cpp:53-60).
const BTN_INSERT_ZEN: (f32, f32) = (POS.0 + 38.0, POS.1 + 390.0);
const BTN_TAKE_ZEN: (f32, f32) = (POS.0 + 78.0, POS.1 + 390.0);
const BTN_LOCK: (f32, f32) = (POS.0 + 118.0, POS.1 + 390.0);
/// The top X (`ProcessBtns`, NewUIStorageInventory.cpp:476-480).
const BTN_CLOSE: (f32, f32, f32, f32) = (POS.0 + 169.0, POS.1 + 7.0, 13.0, 12.0);
/// `CharacterMachine->Gold + iInputZen <= 2000000000` (NewUICustomMessageBox.cpp:4592).
const MAX_ZEN: i64 = 2_000_000_000;

/// `CNewUITextInputMsgBox` / `CNewUICommonMessageBox` (NewUICommonMessageBox.h:32-56 and
/// NewUICustomMessageBox.h:30-40): `MSGBOX_WIDTH`, the heights of the top, a text line and the
/// bottom, the text start and width, the buttons and the input box.
const MSG_W: f32 = 230.0;
const MSG_X: f32 = 640.0 / 2.0 - MSG_W / 2.0;
const MSG_Y: f32 = 100.0;
const MSG_TOP: f32 = 67.0;
const MSG_MIDDLE: f32 = 15.0;
const MSG_BOTTOM: f32 = 50.0;
const MSG_TEXT_TOP: f32 = 35.0;
const MSG_TEXT_MAX: f32 = 180.0;
const MSG_BTN_W: f32 = 54.0;
const MSG_BTN_H: f32 = 30.0;
const MSG_BTN_BOTTOM: f32 = 20.0;
const INPUT_W: f32 = 50.0;
const INPUT_H: f32 = 12.0;
const INPUT_TOP: f32 = 10.0;
/// `INPUTBOX_TEXTLIMIT`.
const INPUT_LIMIT: usize = 8;
/// The caret blink of the text boxes (`m_iCaretBlinkTemp % 24 < 12`, like chat.rs).
const CARET_PERIOD: f32 = 24.0 / 25.0;

/// Texts: `GlobalText[234, 240, 241, 266, 235, 236, 242, 420, 421, 423, 667, 440, 441, 401]`, in
/// English from MuMain's Localization/Game.en.resx (lines 652, 670, 673, 748, 655, 658, 676, 1549,
/// 1552, 1558, 2241, 1609, 1612, 1492; the indices from the call sites MuMain shares with Main 5.2:
/// UI/Inventory/StorageInventory.cpp:258-328, 507-542, 728, 811-858).
const TITLE: &str = "Storage";
const OPEN: &str = "open";
const CLOSED: &str = "close";
const FEE: &str = "Storage fee";
const TIP_INSERT_ZEN: &str = "Deposit";
const TIP_TAKE_ZEN: &str = "Withdraw";
const TIP_LOCK: &str = "Warehouse Lock/Unlock";
const ASK_DEPOSIT: &str = "Enter the amount of Zen you would like to deposit.";
const ASK_WITHDRAW: &str = "Enter the amount of Zen you would like to withdraw.";
const SHORT_OF_ZEN: &str = "You are short of Zen.";
const CANNOT_STORE: &str = "These items cannot be stored in the inventory.";
const INCORRECT_PASSWORD: &str = "Incorrect password";
const ALREADY_LOCKED: &str = "Inventory is already locked";
const WRONG_PASSWORD: &str = "The password you have entered is incorrect.";

// --- State -------------------------------------------------------------------------------------

/// The vault's buttons (`m_abtn`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Button {
    InsertZen,
    TakeZen,
    Lock,
}

/// `m_EventState` of a message box button (`CNewUIMessageBoxButton::Update`,
/// NewUICommonMessageBox.cpp:139-172).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum BtnEvent {
    #[default]
    None,
    Hover,
    Down,
}

impl BtnEvent {
    fn update(self, held: bool, over: bool) -> Self {
        match self {
            BtnEvent::None if !held && over => BtnEvent::Hover,
            BtnEvent::Hover if !held && !over => BtnEvent::None,
            BtnEvent::Hover if held && over => BtnEvent::Down,
            BtnEvent::Down if !held => BtnEvent::None,
            s => s,
        }
    }

    /// The texture row (`m_EventState * m_fButtonHeight` with `KJH_ADD_INGAMESHOP_UI_SYSTEM`).
    fn frame(self) -> f32 {
        match self {
            BtnEvent::None => 0.0,
            BtnEvent::Hover => 1.0,
            BtnEvent::Down => 2.0,
        }
    }
}

/// A message box of the vault.
#[derive(Clone, Debug, PartialEq)]
enum MsgBox {
    /// `CZenReceiptMsgBoxLayout` (deposit) or `CZenPaymentMsgBoxLayout` (`withdraw`): a
    /// `CNewUITextInputMsgBox` of `MSGBOX_COMMON_TYPE_OKCANCEL` with a number box
    /// (`UIOPTION_NUMBERONLY | UIOPTION_PAINTBACK`).
    Zen { withdraw: bool, input: String, ok: BtnEvent, cancel: BtnEvent },
    /// `CreateOkMessageBox(text)`: a `CNewUICommonMessageBox` of `MSGBOX_COMMON_TYPE_OK`.
    Ok { text: &'static str, ok: BtnEvent },
}

/// Where a message box's parts are.
struct BoxLayout {
    h: f32,
    lines: Vec<String>,
    ok: (f32, f32),
    cancel: Option<(f32, f32)>,
    input: Option<(f32, f32)>,
}

/// `SeparateText` (NewUICustomMessageBox.cpp:208-282, NewUICommonMessageBox.cpp:486-558): the text
/// cut, character by character, into lines no wider than `max`.
fn separate_text(text: &str, max: f32, width: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = text.to_string();
    while width(&rest) > max {
        let cut = rest.char_indices().find(|&(i, c)| width(&rest[..i + c.len_utf8()]) > max).map_or(0, |(i, _)| i);
        if cut == 0 {
            break;
        }
        lines.push(rest[..cut].to_string());
        rest = rest[cut..].to_string();
    }
    lines.push(rest);
    lines
}

impl MsgBox {
    fn layout(&self, width: &dyn Fn(&str) -> f32) -> BoxLayout {
        let (x, y) = (MSG_X, MSG_Y);
        // `((GetSize().cx / 2) - MSGBOX_BTN_WIDTH) / 2` in integers.
        let half_gap = ((MSG_W / 2.0 - MSG_BTN_W) / 2.0).floor();
        match self {
            MsgBox::Zen { withdraw, .. } => {
                let lines = separate_text(if *withdraw { ASK_WITHDRAW } else { ASK_DEPOSIT }, MSG_TEXT_MAX, width);
                // `CNewUITextInputMsgBox::Create` (NewUICustomMessageBox.cpp:39-75): top, one line and
                // bottom; `AddMsg` (168-192) adds a line height per extra line and moves the buttons
                // and the input box down with it.
                let extra = (lines.len() as f32 - 1.0).max(0.0) * MSG_MIDDLE;
                let h = MSG_TOP + MSG_MIDDLE + MSG_BOTTOM + extra;
                let by = y + h - (MSG_BTN_H + MSG_BTN_BOTTOM);
                BoxLayout {
                    h,
                    lines,
                    ok: (x + half_gap, by),
                    cancel: Some((x + MSG_W / 2.0 + half_gap, by)),
                    input: Some((x + MSG_W / 2.0 - INPUT_W / 2.0, y + MSG_TOP - INPUT_TOP + extra)),
                }
            }
            MsgBox::Ok { text, .. } => {
                let lines = separate_text(text, MSG_TEXT_MAX, width);
                // `CNewUICommonMessageBox::Create` (NewUICommonMessageBox.cpp:341-361) and `AddMsg`
                // (465-484): two lines fit in top and bottom.
                let h = MSG_TOP + MSG_BOTTOM + (lines.len() as f32 - 2.0).max(0.0) * MSG_MIDDLE;
                BoxLayout { h, lines, ok: (x + MSG_W / 2.0 - MSG_BTN_W / 2.0, y + h - (MSG_BTN_H + MSG_BTN_BOTTOM)), cancel: None, input: None }
            }
        }
    }
}

/// What a message box event asks of the rest of the game.
#[derive(Clone, Debug, PartialEq)]
enum BoxAction {
    /// The box stays (`CALLBACK_CONTINUE`).
    Keep,
    /// `MSGBOX_EVENT_DESTROY` and `SOUND_CLICK01`, sending this, then showing that box.
    Close { send: Option<ClientMessage>, then: Option<MsgBox> },
}

/// `CNewUIStorageInventory`.
#[derive(Resource, Default, Debug)]
pub struct Vault {
    /// `IsVisible(INTERFACE_STORAGE)`.
    pub open: bool,
    /// `CharacterMachine->StorageGold`.
    pub zen: u32,
    /// `m_bLock` and `m_bCorrectPassword`.
    lock: bool,
    correct_password: bool,
    /// The item list the grid was filled from.
    source: Vec<(u8, [u8; 12])>,
    /// [`NpcDialog::serial`] of the dialog shown.
    serial: u32,
    /// Grid `m_EventState` and `m_pToolTipItem` (index into the vault's items).
    event: CtrlEvent,
    tooltip: Option<usize>,
    /// A button held down.
    down: Option<Button>,
    /// The message box on top (`g_MessageBox`) and its manager's `EVENT_WND_MOUSE_LBUTTON_DOWN`.
    msg: Option<MsgBox>,
    msg_armed: bool,
    caret_clock: f32,
}

impl Vault {
    /// `!IsStorageLocked() || IsCorrectPassword()` is false: taking out needs the pin first.
    pub fn needs_pin(&self) -> bool {
        self.lock && !self.correct_password
    }

    /// The mouse (640×480 screen) is over the window: a click there does not walk.
    pub fn blocks(&self, c: Vec2) -> bool {
        self.open && inside(c, POS.0, POS.1, W, H)
    }

    /// `Show(INTERFACE_STORAGE)`.
    fn opening(&mut self, inv: &mut Inventory) {
        self.open = true;
        self.event = CtrlEvent::None;
        self.tooltip = None;
        self.down = None;
        inv.vault = inventory::VaultGrid { open: true, ..Default::default() };
    }

    /// `ProcessClosing` (NewUIStorageInventory.cpp:273-282) without `SendRequestStorageExit` (npc.rs
    /// sends it): `BackupPickedItem` and `DeleteAllItems`. The lock, the password state and the zen
    /// are kept, like the original's members. The vault's message box goes too: it is drawn with the
    /// vault (the original's stays on its own).
    fn closing(&mut self, inv: &mut Inventory) {
        inv.backup_picked();
        inv.vault = inventory::VaultGrid::default();
        *self = Vault { zen: self.zen, lock: self.lock, correct_password: self.correct_password, caret_clock: self.caret_clock, ..Vault::default() };
    }

    /// `InsertItem` for each item of the list.
    fn fill(&mut self, inv: &mut Inventory, list: &[(u8, [u8; 12])]) {
        self.source = list.to_vec();
        inv.vault.items.clear();
        for (slot, bytes) in list {
            if let Some(item) = Item::from_bytes(bytes) {
                inv.vault.insert(*slot, item);
            }
        }
        self.tooltip = None;
        self.event = CtrlEvent::None;
    }

    /// `ProcessToReceiveStorageStatus` (NewUIStorageInventory.cpp:530-596). Returns the text of the OK
    /// box to show, if any.
    fn status(&mut self, state: u8, inv: &mut Inventory) -> Option<&'static str> {
        match state {
            vault_state::UNPROTECTED => (self.lock, self.correct_password) = (false, false),
            vault_state::LOCKED => (self.lock, self.correct_password) = (true, false),
            vault_state::UNLOCK_FAILED_WRONG_PIN => {
                inv.backup_picked();
                inv.vault.auto_move = None;
                return Some(INCORRECT_PASSWORD);
            }
            vault_state::SET_PIN_FAILED_LOCKED => return Some(ALREADY_LOCKED),
            vault_state::UNLOCKED => {
                // The original also sends here what waited for the pin (`m_nBackupTakeZen` or the
                // item of `m_nBackupInvenIndex`); without the keypad nothing waits.
                (self.lock, self.correct_password) = (true, true);
            }
            vault_state::REMOVE_PIN_FAILED_WRONG_PASSWORD => return Some(WRONG_PASSWORD),
            _ => {}
        }
        None
    }

    /// `CreateMessageBox(...)`: one box at a time here.
    fn show_box(&mut self, msg: MsgBox) {
        self.msg = Some(msg);
        self.msg_armed = false;
        self.caret_clock = 0.0;
    }

    /// A message box event: `ReturnDown` / `OkBtnDown` (`ok`) or `CancelBtnDown`, with the zen of the
    /// inventory (`CharacterMachine->Gold`).
    fn box_event(&self, ok: bool, gold: u32) -> BoxAction {
        let close = |send, then| BoxAction::Close { send, then };
        match &self.msg {
            None => BoxAction::Keep,
            // `CNewUICommonMessageBox::Close`.
            Some(MsgBox::Ok { .. }) => close(None, None),
            Some(MsgBox::Zen { .. }) if !ok => close(None, None),
            Some(MsgBox::Zen { withdraw, input, .. }) => {
                // `ProcessOk` (NewUICustomMessageBox.cpp:4509-4538 and 4576-4619): an empty box or 0
                // keeps it open.
                let zen: i64 = input.parse().unwrap_or(0);
                if zen == 0 {
                    return BoxAction::Keep;
                }
                let (gold, stored) = (gold as i64, self.zen as i64);
                if !withdraw {
                    if zen <= gold {
                        close(Some(ClientMessage::VaultMoveMoney { withdraw: false, amount: zen as u32 }), None)
                    } else {
                        close(None, Some(MsgBox::Ok { text: SHORT_OF_ZEN, ok: BtnEvent::None }))
                    }
                } else if zen <= stored && gold + zen <= MAX_ZEN {
                    if self.needs_pin() {
                        // TODO: `SetBackupTakeZen` and the `CPasswordKeyPadMsgBoxLayout` keypad.
                        warn!("baúl: está cerrado con clave; falta el teclado numérico para sacar zen");
                        close(None, None)
                    } else {
                        close(Some(ClientMessage::VaultMoveMoney { withdraw: true, amount: zen as u32 }), None)
                    }
                } else if gold + zen > MAX_ZEN {
                    close(None, None)
                } else {
                    close(None, Some(MsgBox::Ok { text: SHORT_OF_ZEN, ok: BtnEvent::None }))
                }
            }
        }
    }

    /// Carries out a [`BoxAction`].
    fn apply_box(&mut self, action: BoxAction, net: &mut crate::net::Net, sound: &mut SoundQueue) {
        let BoxAction::Close { send, then } = action else { return };
        sound.play("SOUND_CLICK01");
        self.msg = None;
        self.msg_armed = false;
        if let Some(p) = send
            && net.connected()
        {
            info!("baúl: se pide mover zen {p:?}");
            net.send(p);
        }
        if let Some(m) = then {
            self.show_box(m);
        }
    }
}

/// The storage fee (`RenderText`, NewUIStorageInventory.cpp:204-213): `level² · 0.04`, plus twice the
/// level when locked, at least 1, rounded like the prices. (The master level counts too; it is not
/// known yet, so 0.)
fn storage_fee(level: u32, locked: bool) -> i64 {
    let total = level as f64;
    let mut fee = (total * total * 0.04) as i64;
    if locked {
        fee += level as i64 * 2;
    }
    fee = fee.max(1);
    if fee >= 1000 {
        fee / 100 * 100
    } else if fee >= 100 {
        fee / 10 * 10
    } else {
        fee
    }
}

/// What the vault draws this frame, for inventory.rs (which draws it in its layers).
#[derive(Resource, Default)]
pub struct VaultDraw {
    pub back: Vec<Piece>,
    pub front: Vec<Piece>,
    pub views: Vec<ItemView>,
}

// --- Systems -----------------------------------------------------------------------------------

/// The vault follows npc.rs' dialog: it opens (with the inventory) and closes with it, and fills its
/// grid when the item list arrives.
fn follow_dialog(dialog: Res<NpcDialog>, mut vault: ResMut<Vault>, mut inv: ResMut<Inventory>, mut ui: ResMut<crate::options::GameUi>) {
    let is_vault = dialog.kind() == Some(WindowKind::Vault);
    if vault.open && (!is_vault || vault.serial != dialog.serial) {
        vault.closing(&mut inv);
        info!("baúl: se cierra");
    }
    if is_vault && !vault.open {
        vault.opening(&mut inv);
        vault.serial = dialog.serial;
        // `ShowInterface(INTERFACE_INVENTORY)` (NewUISystem.cpp:669).
        inv.show();
        info!("baúl: se abre");
    }
    if vault.open && vault.source != dialog.store {
        let store = dialog.store.clone();
        vault.fill(&mut inv, &store);
        info!("baúl: {} objetos", inv.vault.items.len());
    }
    if ui.msg_box != vault.msg.is_some() {
        ui.msg_box = vault.msg.is_some();
    }
}

/// `ReceiveStorageGold`, `ReceiveStorageExit` and `ReceiveStorageStatus` (WSclient.cpp:6463-6485).
fn server_messages(
    mut from: MessageReader<crate::net::FromServer>,
    mut vault: ResMut<Vault>,
    mut inv: ResMut<Inventory>,
    mut info: Option<ResMut<crate::account::HeroInfo>>,
) {
    for m in from.read() {
        match &m.0 {
            ServerMessage::VaultMoney { success, vault: stored, inventory } => {
                // `if(Data->Result)`: `StorageGold` and `Gold`.
                if *success {
                    vault.zen = *stored;
                    match info.as_mut() {
                        Some(info) => info.0.money = *inventory,
                        None => inv.zen = *inventory,
                    }
                }
            }
            ServerMessage::VaultClosed => info!("baúl: el servidor confirma el cierre"),
            ServerMessage::VaultProtection { state } => {
                if let Some(text) = vault.status(*state, &mut inv) {
                    vault.show_box(MsgBox::Ok { text, ok: BtnEvent::None });
                }
                info!("baúl: estado de la clave {state}");
            }
            _ => {}
        }
    }
}

/// The message box's keys (`CNewUIMessageBoxMng::UpdateKeyEvent`, NewUIMessageBox.cpp:275-298, and the
/// number box): while it is up nothing else gets a key.
#[allow(clippy::too_many_arguments)]
fn keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    time: Res<Time>,
    mut vault: ResMut<Vault>,
    mut ui: ResMut<crate::options::GameUi>,
    inv: Res<Inventory>,
    info: Option<Res<crate::account::HeroInfo>>,
    mut net: ResMut<crate::net::Net>,
    mut sound: ResMut<SoundQueue>,
) {
    let typed: Vec<Key> = crate::clipboard::typed_keys(typed.read().cloned(), &keys);
    vault.caret_clock += time.delta_secs();
    let Some(msg) = vault.msg.clone() else { return };
    let esc = keys.just_pressed(KeyCode::Escape);
    let enter = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    let gold = info.as_ref().map_or(inv.zen, |i| i.0.money);
    match msg {
        MsgBox::Zen { .. } => {
            // The number box: digits only, at most `INPUTBOX_TEXTLIMIT`.
            if let Some(MsgBox::Zen { input, .. }) = vault.msg.as_mut() {
                for key in &typed {
                    match key {
                        Key::Character(t) => {
                            for c in t.chars().filter(char::is_ascii_digit) {
                                if input.len() < INPUT_LIMIT {
                                    input.push(c);
                                }
                            }
                        }
                        Key::Backspace => {
                            input.pop();
                        }
                        _ => {}
                    }
                }
            }
            // `MSGBOX_EVENT_PRESSKEY_RETURN` → `ReturnDown`; Esc has no callback in these layouts.
            if enter {
                let action = vault.box_event(true, gold);
                vault.apply_box(action, &mut net, &mut sound);
            }
        }
        // `CNewUICommonMessageBox::Close` on `MSGBOX_EVENT_PRESSKEY_ESC` (Return is commented out).
        MsgBox::Ok { .. } => {
            if esc {
                vault.apply_box(BoxAction::Close { send: None, then: None }, &mut net, &mut sound);
            }
        }
    }
    if ui.msg_box != vault.msg.is_some() {
        ui.msg_box = vault.msg.is_some();
    }
    keys.clear();
}

/// `GetSquareIndexAtPt` of the vault grid.
fn square_at(p: Vec2) -> Option<usize> {
    if !inside(p, GRID.0, GRID.1, VAULT_COLUMNS as f32 * SQUARE, VAULT_ROWS as f32 * SQUARE) {
        return None;
    }
    let (x, y) = (((p.x - GRID.0) / SQUARE) as usize, ((p.y - GRID.1) / SQUARE) as usize);
    Some(y * VAULT_COLUMNS + x)
}

/// `GetTargetLinealPos(m_pNewInventoryCtrl)` (NewUIInventoryCtrl.cpp:108-132): the vault cell under
/// the held item's top-left square (the mouse minus half the item).
fn target_cell(m: Vec2, size: (u8, u8)) -> Option<(u8, u8)> {
    let (w, h) = size;
    let tx = m.x - ((w as i32 - 1) * SQUARE as i32 / 2) as f32;
    let ty = m.y - ((h as i32 - 1) * SQUARE as i32 / 2) as f32;
    let t = square_at(Vec2::new(tx, ty))?;
    Some(((t % VAULT_COLUMNS) as u8, (t / VAULT_COLUMNS) as u8))
}

/// `SendRequestItemToStorage` (NewUIStorageInventory.cpp:426-448): an inventory item into vault slot
/// `to`, unless `IsStoreBan`. `item`/`from_slot` are the held item or, for a right click, the one at
/// that inventory slot. Returns the error text to show in the chat.
fn request_to_storage(inv: &mut Inventory, net: &mut crate::net::Net, item: Item, from_slot: u8, to: u8) -> Option<&'static str> {
    if price::is_store_ban(&item) {
        inv.backup_picked();
        inv.vault.auto_move = None;
        return Some(CANNOT_STORE);
    }
    inv.send_move(net, storage::INVENTORY, from_slot, item, storage::VAULT, to);
    None
}

/// `CNewUIStorageInventory::UpdateMouseEvent` (NewUIStorageInventory.cpp:90-117) with its grid's
/// `UpdateProcess` and `UpdateMouseEvent` (NewUIInventoryCtrl.cpp:785-860), and the message box on
/// top of everything (`CNewUIMessageBoxMng::UpdateMouseEvent`, NewUIMessageBox.cpp:196-273).
#[allow(clippy::too_many_arguments)]
fn update_mouse(
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    data: Res<ItemData>,
    game_ui: Res<crate::options::GameUi>,
    info: Option<Res<crate::account::HeroInfo>>,
    (mut chat, metrics): (ResMut<crate::chat::Chat>, Res<crate::chat::TextMetrics>),
    mut vault: ResMut<Vault>,
    (mut dialog, mut inv, mut net): (ResMut<NpcDialog>, ResMut<Inventory>, ResMut<crate::net::Net>),
    mut sound: ResMut<SoundQueue>,
) {
    if !vault.open {
        return;
    }
    let Some(m) = windows.single().ok().and_then(crate::mainframe::cursor_virtual) else { return };
    let m = m.floor();
    let (pressed, released, just_pressed) =
        (mouse.pressed(MouseButton::Left), mouse.just_released(MouseButton::Left), mouse.just_pressed(MouseButton::Left));
    let vault = &mut *vault;

    // The message box.
    if let Some(l) = vault.msg.as_ref().map(|msg| msg.layout(&|s: &str| metrics.width(s))) {
        let over = |b: (f32, f32)| inside(m, b.0, b.1, MSG_BTN_W, MSG_BTN_H);
        match vault.msg.as_mut() {
            Some(MsgBox::Zen { ok, cancel, .. }) => {
                *ok = ok.update(pressed, over(l.ok));
                *cancel = cancel.update(pressed, l.cancel.is_some_and(over));
            }
            Some(MsgBox::Ok { ok, .. }) => *ok = ok.update(pressed, over(l.ok)),
            None => {}
        }
        let in_box = inside(m, MSG_X, MSG_Y, MSG_W, l.h);
        if just_pressed && in_box {
            vault.msg_armed = true;
        }
        if released && std::mem::take(&mut vault.msg_armed) && in_box {
            // `LButtonUp` (NewUICustomMessageBox.cpp:135-166, NewUICommonMessageBox.cpp:664-695).
            let gold = info.as_ref().map_or(inv.zen, |i| i.0.money);
            let action = if over(l.ok) {
                vault.box_event(true, gold)
            } else if l.cancel.is_some_and(over) {
                vault.box_event(false, gold)
            } else {
                BoxAction::Keep
            };
            vault.apply_box(action, &mut net, &mut sound);
        }
        return;
    }
    if game_ui.modal() {
        return;
    }

    // The grid's `UpdateProcess` and `UpdateMouseEvent`: hover, tooltip and picking up on release.
    let pointed = square_at(m).and_then(|s| inv.vault.owner(&data, s % VAULT_COLUMNS, s / VAULT_COLUMNS));
    if pointed.is_none() {
        vault.event = CtrlEvent::None;
        vault.tooltip = None;
    }
    if vault.event == CtrlEvent::None && !pressed && pointed.is_some() {
        vault.event = CtrlEvent::Hover;
    } else if vault.event == CtrlEvent::Hover && released && inv.picked.is_none() {
        if let Some(i) = pointed {
            vault.event = CtrlEvent::Picking;
            let g = inv.vault.items.remove(i);
            inv.picked = Some(Picked { item: g.item, from: Source::Vault { x: g.x, y: g.y } });
            vault.tooltip = None;
            return;
        }
    } else if vault.event == CtrlEvent::Hover && !pressed && inv.picked.is_none() && pointed != vault.tooltip {
        vault.tooltip = pointed;
    }

    let online = net.connected();
    let mut chat_error = |text: &str| {
        chat.add("", text, MsgType::Error, MsgType::All, &metrics);
    };
    // `ProcessInventoryCtrl`: the held item pressed over a free place of the vault.
    if let Some(p) = inv.picked {
        if just_pressed
            && online
            && let Some((x, y)) = target_cell(m, Inventory::size(&data, &p.item))
        {
            let (w, h) = Inventory::size(&data, &p.item);
            if inv.vault.fits(&data, x as i32, y as i32, w, h) {
                let to = y * VAULT_COLUMNS as u8 + x;
                match p.from {
                    Source::Vault { .. } => {
                        inv.request_move_to(&mut net, storage::VAULT, to);
                    }
                    Source::Grid { .. } | Source::Equipment(_) => {
                        if let Some(text) = request_to_storage(&mut inv, &mut net, p.item, inventory::source_slot(p.from), to) {
                            chat_error(text);
                        }
                    }
                }
            }
        }
    } else if mouse.just_pressed(MouseButton::Right) && online && !inv.busy() && inv.vault.auto_move.is_none() {
        if let Some(i) = pointed {
            // `ProcessStorageItemAutoMove` (NewUIStorageInventory.cpp:349-374): to the first free place
            // of the inventory. (The original raises `m_bItemAutoMove` even when the request is not
            // sent because another move is pending; here it waits for that one.)
            let g = inv.vault.items[i];
            if let Some((ix, iy)) = inv.find_empty(&data, &g.item) {
                if vault.needs_pin() {
                    // TODO: the `CPasswordKeyPadMsgBoxLayout` keypad (its Cancel: `SetItemAutoMove(false)`).
                    warn!("baúl: está cerrado con clave; falta el teclado numérico para sacar objetos");
                } else if inv.send_move(&mut net, storage::VAULT, g.y * VAULT_COLUMNS as u8 + g.x, g.item, storage::INVENTORY, inventory::grid_slot(ix, iy)) {
                    inv.vault.auto_move = Some(Source::Vault { x: g.x, y: g.y });
                }
                sound.play("SOUND_GET_ITEM01");
            }
        } else if !inv.repair_mode
            && let Some(i) = inv.grid_item_at(&data, m)
        {
            // `ProcessMyInvenItemAutoMove` (NewUIStorageInventory.cpp:376-405), from the inventory's
            // right click (NewUIMyInventory.cpp:1767-1774): not the Wizard's Ring (`ITEM_HELPER+20`).
            let g = inv.grid[i];
            if g.item.kind() != mu_data::items::kind(group::HELPER, 20)
                && let Some(to) = inv.vault.find_empty(&data, &g.item)
            {
                if price::is_store_ban(&g.item) {
                    chat_error(CANNOT_STORE);
                } else if inv.send_move(&mut net, storage::INVENTORY, inventory::grid_slot(g.x, g.y), g.item, storage::VAULT, to) {
                    inv.vault.auto_move = Some(Source::Grid { x: g.x, y: g.y });
                }
                sound.play("SOUND_GET_ITEM01");
            }
        }
    }

    // `ProcessBtns` (NewUIStorageInventory.cpp:450-483).
    if just_pressed && inside(m, BTN_CLOSE.0, BTN_CLOSE.1, BTN_CLOSE.2, BTN_CLOSE.3) {
        crate::npc::close_dialog(&mut dialog, &mut net, &mut inv, "X del baúl");
        return;
    }
    let over = |b: (f32, f32)| inside(m, b.0, b.1, BTN_W, BTN_H);
    let under = [(Button::InsertZen, BTN_INSERT_ZEN), (Button::TakeZen, BTN_TAKE_ZEN), (Button::Lock, BTN_LOCK)]
        .into_iter()
        .find(|&(_, b)| over(b))
        .map(|(k, _)| k);
    if just_pressed {
        vault.down = under;
    }
    if !pressed {
        if released && vault.down.is_some() && vault.down == under {
            match under {
                Some(Button::InsertZen) => vault.show_box(MsgBox::Zen { withdraw: false, input: String::new(), ok: BtnEvent::None, cancel: BtnEvent::None }),
                Some(Button::TakeZen) => vault.show_box(MsgBox::Zen { withdraw: true, input: String::new(), ok: BtnEvent::None, cancel: BtnEvent::None }),
                // TODO: `CStorageUnlockMsgBoxLayout` (locked) / `CStorageLockKeyPadMsgBoxLayout`.
                Some(Button::Lock) => warn!("baúl: falta la ventana para poner o quitar la clave"),
                None => {}
            }
        }
        vault.down = None;
    }
}

/// `Render` (NewUIStorageInventory.cpp:141-218) and the message box into [`VaultDraw`].
#[allow(clippy::too_many_arguments)]
fn prepare_draw(
    windows: Query<&Window>,
    data: Res<ItemData>,
    metrics: Res<crate::chat::TextMetrics>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    inv: Res<Inventory>,
    vault: Res<Vault>,
    mut draw: ResMut<VaultDraw>,
) {
    if !vault.open {
        if !draw.back.is_empty() || !draw.front.is_empty() || !draw.views.is_empty() {
            *draw = VaultDraw::default();
        }
        return;
    }
    let m = windows.single().ok().and_then(crate::mainframe::cursor_virtual).map(|c| c.floor());
    let level = hero.as_ref().map_or(1, |h| h.level);
    let view = inventory::hero_view(hero.as_deref(), info.as_deref());
    let (back, front, views) = vault_pieces(&vault, &inv, &data, &view, level, m, &metrics);
    if draw.back != back {
        draw.back = back;
    }
    if draw.front != front {
        draw.front = front;
    }
    if draw.views != views {
        draw.views = views;
    }
}

/// The vault's background, tooltips, message box and 3D items.
#[allow(clippy::too_many_arguments)]
fn vault_pieces(
    vault: &Vault,
    inv: &Inventory,
    data: &ItemData,
    hero: &mu_data::items::info::HeroView,
    level: u32,
    m: Option<Vec2>,
    metrics: &crate::chat::TextMetrics,
) -> (Vec<Piece>, Vec<Piece>, Vec<ItemView>) {
    let mut back = Vec::new();
    let (x, y) = POS;
    // `RenderBackImage` (161-176).
    back.push(inventory::img(Tex::Back, x, y, W, H, 0.0));
    back.push(inventory::img(Tex::StorageTop, x, y, W, 64.0, 0.0));
    back.push(inventory::img(Tex::Left, x, y + 64.0, 21.0, 320.0, 0.0));
    back.push(inventory::img(Tex::Right, x + W - 21.0, y + 64.0, 21.0, 320.0, 0.0));
    back.push(inventory::img(Tex::Bottom, x, y + H - 45.0, W, 45.0, 0.0));
    back.push(inventory::img(Tex::StorageMoney, x + 10.0, y + 342.0, 170.0, 46.0, 0.0));
    // `RenderText` (178-218), all with `g_hFontBold`: the title red when locked, the vault's zen and
    // the storage fee right-aligned at x + 168.
    let rgb = |r: u8, g: u8, b: u8| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
    let title = format!("{TITLE} ({})", if vault.lock { CLOSED } else { OPEN });
    let title_color = if vault.lock { rgb(240, 32, 32) } else { rgb(216, 216, 216) };
    back.push(inventory::text(x, y + 11.0, Some(W), &title, title_color, true));
    let right = |s: String, ty: f32, color: [f32; 3]| Piece::TextRight { x: x + 168.0, y: ty, text: s, color, bold: true };
    back.push(right(inventory::convert_gold(vault.zen), y + 342.0 + 8.0, inventory::gold_color(vault.zen)));
    back.push(inventory::text(x + 10.0 + 15.0, y + 342.0 + 29.0, None, FEE, rgb(240, 64, 64), true));
    back.push(right(price::convert_gold(storage_fee(level, vault.lock)), y + 342.0 + 29.0, rgb(255, 220, 150)));

    // The grid (`m_pNewInventoryCtrl->Render`), with the held item's landing cells: red for an item
    // that cannot be stored (`LEM_ADD_LUCKYITEM`, NewUIStorageInventory.cpp:337-340).
    let occupied = |cx: usize, cy: usize| inv.vault.owner(data, cx, cy).is_some();
    inventory::push_grid_frame(&mut back, GRID, VAULT_COLUMNS, VAULT_ROWS, &occupied);
    if let (Some(p), Some(m)) = (inv.picked, m) {
        let normal = if price::is_store_ban(&p.item) { [1.0, 0.0, 0.0] } else { DROP_NORMAL };
        inventory::push_drop_overlay(&mut back, GRID, VAULT_COLUMNS, VAULT_ROWS, Inventory::size(data, &p.item), m, &occupied, normal);
    }

    // The buttons (`m_abtn[i].Render()`): the lock one shows the lock state (`ChangeLockBtnImage`).
    let over = |b: (f32, f32)| m.is_some_and(|m| inside(m, b.0, b.1, BTN_W, BTN_H));
    let lock_tex = if vault.lock { Tex::BtnLock } else { Tex::BtnUnlock };
    let mut tips = Vec::new();
    for (button, pos, tex, tip) in [
        (Button::InsertZen, BTN_INSERT_ZEN, Tex::BtnInsertZen, TIP_INSERT_ZEN),
        (Button::TakeZen, BTN_TAKE_ZEN, Tex::BtnTakeZen, TIP_TAKE_ZEN),
        (Button::Lock, BTN_LOCK, lock_tex, TIP_LOCK),
    ] {
        let frame = if vault.down == Some(button) && over(pos) { BTN_H } else { 0.0 };
        back.push(inventory::img(tex, pos.0, pos.1, BTN_W, BTN_H, frame));
        if over(pos) && vault.msg.is_none() {
            tips.push((tip, pos));
        }
    }
    for (s, (bx, by)) in tips {
        inventory::push_button_tip(&mut back, s, bx, by, metrics);
    }

    // The item tooltip (`RenderItemToolTip`, `TOOLTIP_TYPE_INVENTORY`).
    let mut front = Vec::new();
    if let (Some(i), None, None) = (vault.tooltip, inv.picked, &vault.msg)
        && let Some(g) = inv.vault.items.get(i)
        && let Some(attr) = data.attr(&g.item)
    {
        let mana = |s: u16| data.skill_mana.as_ref().and_then(|m| m.get(s as usize)).map(|&m| m as u32);
        let lines = item_info(attr, &g.item, hero, &mana);
        let sx = GRID.0 + g.x as f32 * SQUARE + attr.width as f32 * SQUARE / 2.0;
        let mut sy = GRID.1 + g.y as f32 * SQUARE;
        if attr.height == 1 {
            sy += SQUARE / 2.0;
        }
        inventory::push_tooltip(&mut front, &lines, sx.floor(), sy, metrics);
    }
    if let Some(msg) = &vault.msg {
        push_msg_box(&mut front, msg, vault.caret_clock, metrics);
    }

    // `CNewUIInventoryCtrl::Render3D`: spins when hovered.
    let free = inv.picked.is_none() && vault.msg.is_none();
    let views = inv
        .vault
        .items
        .iter()
        .map(|g: &GridItem| {
            let (w, h) = Inventory::size(data, &g.item);
            let rect = [GRID.0 + g.x as f32 * SQUARE, GRID.1 + g.y as f32 * SQUARE, w as f32 * SQUARE, h as f32 * SQUARE];
            let hovered = free && m.is_some_and(|m| inside(m, rect[0], rect[1], rect[2], rect[3]));
            ItemView { key: 3000 + g.y as u32 * VAULT_COLUMNS as u32 + g.x as u32, item: g.item, rect, picked: false, hovered }
        })
        .collect();
    (back, front, views)
}

/// `CNewUITextInputMsgBox::Render` (NewUICustomMessageBox.cpp:319-403) and
/// `CNewUICommonMessageBox::Render` (NewUICommonMessageBox.cpp:576-657).
fn push_msg_box(out: &mut Vec<Piece>, msg: &MsgBox, caret_clock: f32, metrics: &crate::chat::TextMetrics) {
    let l = msg.layout(&|s: &str| metrics.width(s));
    let (x, y) = (MSG_X, MSG_Y);
    // `MSGBOX_BACK_BLANK_WIDTH/HEIGHT` = 8 and 10.
    out.push(inventory::img(Tex::Back, x, y + 2.0, MSG_W - 8.0, l.h - 10.0, 0.0));
    out.push(inventory::img(Tex::MsgTop, x, y, MSG_W, MSG_TOP, 0.0));
    let middles = (l.h - MSG_TOP - MSG_BOTTOM) / MSG_MIDDLE;
    for i in 0..middles.round() as usize {
        out.push(inventory::img(Tex::MsgMiddle, x, y + MSG_TOP + MSG_MIDDLE * i as f32, MSG_W, MSG_MIDDLE, 0.0));
    }
    out.push(inventory::img(Tex::MsgBottom, x, y + l.h - MSG_BOTTOM, MSG_W, MSG_BOTTOM, 0.0));
    // `RenderTexts`: white `g_hFont`, centered, one line every text height + 4.
    let mut ty = y + MSG_TEXT_TOP;
    for line in &l.lines {
        let tx = (x + MSG_W / 2.0 - (metrics.width(line) / 2.0).floor()).floor();
        out.push(inventory::text(tx, ty, None, line, [1.0; 3], false));
        ty += crate::chat::FONT + 4.0;
    }
    // The number box (`CUITextInputBox::Render`, UIControls.cpp:3470-3493): black background
    // (`UIOPTION_PAINTBACK`), the text in (255, 230, 210) and the caret.
    if let (Some((ix, iy)), MsgBox::Zen { input, .. }) = (l.input, msg) {
        out.push(inventory::color(ix, iy, INPUT_W, INPUT_H, [0.0, 0.0, 0.0, 1.0]));
        out.push(inventory::text(ix, iy, None, input, [1.0, 230.0 / 255.0, 210.0 / 255.0], false));
        if caret_clock % CARET_PERIOD < CARET_PERIOD / 2.0 {
            let cx = ix + metrics.width(input).ceil();
            out.push(inventory::color(cx, iy, 2.0, crate::chat::FONT, [200.0 / 255.0, 200.0 / 255.0, 200.0 / 255.0, 1.0]));
        }
    }
    // `CNewUIMessageBoxButton::Render`: 54×30 frames (normal, hover, pressed).
    let button = |out: &mut Vec<Piece>, tex: Tex, (bx, by): (f32, f32), state: BtnEvent| {
        out.push(Piece::Image { tex, x: bx, y: by, w: MSG_BTN_W, h: MSG_BTN_H, src: [0.0, state.frame() * MSG_BTN_H, MSG_BTN_W, MSG_BTN_H] });
    };
    match msg {
        MsgBox::Zen { ok, cancel, .. } => {
            button(out, Tex::MsgOk, l.ok, *ok);
            if let Some(c) = l.cancel {
                button(out, Tex::MsgCancel, c, *cancel);
            }
        }
        MsgBox::Ok { ok, .. } => button(out, Tex::MsgOk, l.ok, *ok),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kris(level: u8) -> Item {
        Item { level, durability: 30, ..Item::new(group::SWORD, 1) }
    }

    /// Opening fills the grid from the list (slot `y · 8 + x`; past 119 is dropped); closing puts the
    /// held item back and empties it.
    #[test]
    fn open_fill_and_close() {
        let mut vault = Vault::default();
        let mut inv = Inventory::default();
        vault.opening(&mut inv);
        assert!(inv.vault.open);
        vault.fill(&mut inv, &[(33, kris(1).to_bytes()), (119, kris(2).to_bytes()), (120, kris(3).to_bytes())]);
        assert_eq!(inv.vault.items, vec![GridItem { x: 1, y: 4, item: kris(1) }, GridItem { x: 7, y: 14, item: kris(2) }]);
        // Picked up from the vault: the source slot and storage of the move.
        let g = inv.vault.items.remove(0);
        inv.picked = Some(Picked { item: g.item, from: Source::Vault { x: g.x, y: g.y } });
        assert_eq!((inventory::source_storage(inv.picked.unwrap().from), inventory::source_slot(inv.picked.unwrap().from)), (storage::VAULT, 33));
        vault.zen = 5;
        vault.closing(&mut inv);
        assert!(!vault.open && !inv.vault.open && inv.vault.items.is_empty() && inv.picked.is_none());
        assert_eq!(vault.zen, 5, "StorageGold is kept");
        assert!(vault.source.is_empty(), "the next opening fills it again");
    }

    /// A move into the vault waits for the server: accepted, the item appears in the vault slot it
    /// says; refused, it goes back to the inventory.
    #[test]
    fn moves_wait_for_the_server() {
        let mut inv = Inventory::default();
        inv.open = true;
        inv.vault.open = true;
        inv.picked = Some(Picked { item: kris(5), from: Source::Grid { x: 2, y: 3 } });
        let mut net = crate::net::Net::offline();
        assert!(inv.request_move_to(&mut net, storage::VAULT, 119));
        assert!(inv.busy());
        // Nothing else moves meanwhile, and the vault (with the inventory) does not close.
        assert!(!inv.request_move_to(&mut net, storage::VAULT, 0));
        inv.toggle();
        assert!(inv.open && inv.picked.is_some(), "ProcessClosing refuses while EquipmentItem");
        inv.apply(&ServerMessage::ItemMoved { storage: storage::VAULT, slot: 119, item: kris(5).to_bytes() });
        assert!(!inv.busy() && inv.picked.is_none());
        assert_eq!(inv.vault.items, vec![GridItem { x: 7, y: 14, item: kris(5) }]);
        assert!(inv.grid.is_empty());

        // Out again and refused: back into the vault.
        let g = inv.vault.items.remove(0);
        inv.picked = Some(Picked { item: g.item, from: Source::Vault { x: g.x, y: g.y } });
        assert!(inv.request_move_to(&mut net, storage::INVENTORY, 12));
        inv.apply(&ServerMessage::ItemMoveFailed);
        assert!(!inv.busy() && inv.picked.is_none());
        assert_eq!(inv.vault.items, vec![GridItem { x: 7, y: 14, item: kris(5) }]);
        // Accepted: into the inventory slot the server says.
        let g = inv.vault.items.remove(0);
        inv.picked = Some(Picked { item: g.item, from: Source::Vault { x: g.x, y: g.y } });
        inv.request_move_to(&mut net, storage::INVENTORY, 12 + 8 + 1);
        inv.apply(&ServerMessage::ItemMoved { storage: storage::INVENTORY, slot: 12 + 8 + 1, item: kris(5).to_bytes() });
        assert_eq!(inv.grid, vec![GridItem { x: 1, y: 1, item: kris(5) }]);
        assert!(inv.vault.items.is_empty());
    }

    /// A right click leaves the item in its grid until the server says where it went.
    #[test]
    fn auto_move() {
        let mut inv = Inventory::default();
        inv.vault.open = true;
        inv.grid.push(GridItem { x: 0, y: 0, item: kris(1) });
        let mut net = crate::net::Net::offline();
        assert!(inv.send_move(&mut net, storage::INVENTORY, 12, kris(1), storage::VAULT, 5));
        inv.vault.auto_move = Some(Source::Grid { x: 0, y: 0 });
        assert_eq!(inv.grid.len(), 1, "still in the inventory");
        inv.apply(&ServerMessage::ItemMoved { storage: storage::VAULT, slot: 5, item: kris(1).to_bytes() });
        assert!(inv.grid.is_empty() && inv.vault.auto_move.is_none());
        assert_eq!(inv.vault.items, vec![GridItem { x: 5, y: 0, item: kris(1) }]);
        // Back with a right click, refused: nothing moves.
        assert!(inv.send_move(&mut net, storage::VAULT, 5, kris(1), storage::INVENTORY, 12));
        inv.vault.auto_move = Some(Source::Vault { x: 5, y: 0 });
        inv.apply(&ServerMessage::ItemMoveFailed);
        assert!(inv.vault.auto_move.is_none() && inv.vault.items.len() == 1 && inv.grid.is_empty());
        // Accepted.
        inv.send_move(&mut net, storage::VAULT, 5, kris(1), storage::INVENTORY, 12);
        inv.vault.auto_move = Some(Source::Vault { x: 5, y: 0 });
        inv.apply(&ServerMessage::ItemMoved { storage: storage::INVENTORY, slot: 12, item: kris(1).to_bytes() });
        assert!(inv.vault.items.is_empty());
        assert_eq!(inv.grid, vec![GridItem { x: 0, y: 0, item: kris(1) }]);
    }

    /// The zen boxes' OK (`ProcessOk`): what is sent, refused or kept open.
    #[test]
    fn zen_boxes() {
        let zen_box = |withdraw: bool, input: &str| MsgBox::Zen { withdraw, input: input.into(), ok: BtnEvent::None, cancel: BtnEvent::None };
        let mut vault = Vault { open: true, zen: 1_000, ..Default::default() };
        vault.show_box(zen_box(false, ""));
        assert_eq!(vault.box_event(true, 500), BoxAction::Keep, "empty");
        vault.show_box(zen_box(false, "0"));
        assert_eq!(vault.box_event(true, 500), BoxAction::Keep, "zero");
        vault.show_box(zen_box(false, "500"));
        assert_eq!(
            vault.box_event(true, 500),
            BoxAction::Close { send: Some(ClientMessage::VaultMoveMoney { withdraw: false, amount: 500 }), then: None }
        );
        vault.show_box(zen_box(false, "501"));
        assert_eq!(vault.box_event(true, 500), BoxAction::Close { send: None, then: Some(MsgBox::Ok { text: SHORT_OF_ZEN, ok: BtnEvent::None }) });
        assert_eq!(vault.box_event(false, 500), BoxAction::Close { send: None, then: None }, "Cancel");
        // Withdrawing: from the vault's zen, not past 2 000 000 000 in the inventory (silently).
        vault.show_box(zen_box(true, "1000"));
        assert_eq!(
            vault.box_event(true, 0),
            BoxAction::Close { send: Some(ClientMessage::VaultMoveMoney { withdraw: true, amount: 1000 }), then: None }
        );
        assert_eq!(vault.box_event(true, 1_999_999_500), BoxAction::Close { send: None, then: None });
        vault.show_box(zen_box(true, "1001"));
        assert_eq!(vault.box_event(true, 0), BoxAction::Close { send: None, then: Some(MsgBox::Ok { text: SHORT_OF_ZEN, ok: BtnEvent::None }) });
        // Locked without the pin: nothing is sent (no keypad yet).
        vault.status(vault_state::LOCKED, &mut Inventory::default());
        vault.show_box(zen_box(true, "10"));
        assert_eq!(vault.box_event(true, 0), BoxAction::Close { send: None, then: None });
    }

    #[test]
    fn lock_states_and_fee() {
        let mut vault = Vault::default();
        let mut inv = Inventory::default();
        assert_eq!(vault.status(vault_state::LOCKED, &mut inv), None);
        assert!(vault.needs_pin());
        assert_eq!(vault.status(vault_state::UNLOCKED, &mut inv), None);
        assert!(vault.lock && !vault.needs_pin());
        assert_eq!(vault.status(vault_state::UNLOCK_FAILED_WRONG_PIN, &mut inv), Some(INCORRECT_PASSWORD));
        assert_eq!(vault.status(vault_state::UNPROTECTED, &mut inv), None);
        assert!(!vault.lock && !vault.needs_pin());
        // level² · 0.04 (+ 2 · level locked), rounded.
        assert_eq!(storage_fee(1, false), 1);
        assert_eq!(storage_fee(10, false), 4);
        assert_eq!(storage_fee(60, false), 140);
        assert_eq!(storage_fee(60, true), 260);
        assert_eq!(storage_fee(400, false), 6400);
    }

    #[test]
    fn grid_and_text() {
        assert_eq!(square_at(Vec2::new(GRID.0 + 1.0, GRID.1 + 1.0)), Some(0));
        assert_eq!(square_at(Vec2::new(GRID.0 + 159.0, GRID.1 + 299.0)), Some(119));
        assert_eq!(square_at(Vec2::new(GRID.0 + 160.0, GRID.1)), None);
        // A 2×3 item: its top-left square is half an item up and left of the mouse.
        assert_eq!(target_cell(Vec2::new(GRID.0 + 25.0, GRID.1 + 45.0), (2, 3)), Some((0, 1)));
        // `SeparateText`: cut where the next character would pass the width.
        let width = |s: &str| s.chars().count() as f32 * 10.0;
        assert_eq!(separate_text("abcdefghij", 40.0, &width), ["abcd", "efgh", "ij"]);
        assert_eq!(separate_text("abc", 40.0, &width), ["abc"]);
        let l = MsgBox::Zen { withdraw: false, input: String::new(), ok: BtnEvent::None, cancel: BtnEvent::None }.layout(&|s: &str| s.len() as f32 * 5.0);
        // 50 characters at 5 = 250 > 180: two lines, one line height more.
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.h, 67.0 + 15.0 + 50.0 + 15.0);
        assert_eq!(l.ok, (MSG_X + 30.0, MSG_Y + l.h - 50.0));
        assert_eq!(l.input, Some((MSG_X + 90.0, MSG_Y + 57.0 + 15.0)));
    }
}
