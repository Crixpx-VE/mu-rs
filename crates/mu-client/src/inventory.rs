//! Inventory (`V` or `I`): `SEASON3B::CNewUIMyInventory` (NewUIMyInventory.cpp) with its 8×8
//! `CNewUIInventoryCtrl` grid (NewUIInventoryCtrl.cpp).
//!
//! - **Open and close**: `I` or `V` (`CNewUIHotKey::UpdateKeyEvent`, NewUIHotKey.cpp:155-163 and 198-204,
//!   with `SOUND_CLICK01`), the bar's inventory button (NewUIMainFrameWindow.cpp:769-775), the bottom
//!   close button and Esc (`UpdateKeyEvent`, NewUIMyInventory.cpp:560-570). On close, the item held by
//!   the mouse returns to its place (`ClosingProcess` → `BackupPickedItem`).
//! - **Drawing**: `Create(..., 640 - 190, 0)` (NewUISystem.cpp:172), `RenderFrame`, `RenderInventoryDetails`,
//!   `RenderSetOption`/`RenderSocketOption`, `RenderButtons`, the grid (`CNewUIInventoryCtrl::Render`) and
//!   the equipment slots (`SetEquipmentSlotInfo`, `RenderEquippedItem`). Items are drawn in 3D
//!   (item_view.rs) on top; tooltips on top of everything.
//! - **Mouse**: pick up an item on button release over it (`CNewUIInventoryCtrl::UpdateMouseEvent`) and
//!   drop it on release over a cell where it fits (`InventoryProcess`) or over a free equipment slot
//!   where it can be worn (`EquipmentWindowProcess` + `IsEquipable`). Picking up is local only: the
//!   server still has the item in its slot. Dropping sends `SendRequestEquipmentItem` (C3 24) and the
//!   item stays on the mouse until the server answers (`EquipmentItem` flag: one request at a time,
//!   `BackupPickedItem` does nothing meanwhile): `ItemMoved` puts it where the server says,
//!   `ItemMoveFailed` sends it back (`ReceiveEquipmentItem`, WSclient.cpp:5693-5797). The server is the
//!   only authority over items (AGENTS.md); only without a connection (`--inventario-prueba`) are moves
//!   applied locally. With an item held, a click in the game asks to drop it there (ground.rs); it
//!   leaves the mouse when the server says so (`ReceiveDropItem`).
//! - **Contents**: the server sends the whole inventory after entering the world (`ReceiveInventory`,
//!   C4 F3 10, WSclient.cpp:1270-1320), removes items (`ReceiveDeleteInventory`, 0x28) and changes
//!   durability or stack size (`ReceiveDurability`, 0x2A). `--inventario-prueba` sets up a test
//!   inventory (+0 to +15, excellent and one ancient) when there is no server.
//!
//! - **Repair** (`SetRepairMode`, NewUIMyInventory.cpp:427-445): the repair button (from level 50,
//!   hidden with the NPC shop, vault... open, 1301-1327) or `L` (573-586) toggle it, and so does the
//!   NPC shop's repair button (shop.rs). In repair mode items are not picked up, the tooltips show
//!   the repair cost (`RenderRepairInfo`) and a click on an item asks the server to repair it
//!   (`SendRequestRepair`, 1463-1487 and 2148-2173): with a repair shop open at its price, otherwise
//!   paying the self-repair price (`p_AddGold` 1). Durability and zen change when the server says so.
//! - **NPC shop**: with it open, the tooltips show the selling price (`RenderItemInfo`,
//!   ZzzInventory.cpp:2189-2213) and an item dropped on the shop is sold (shop.rs).
//! - **Vault** (vault.rs): its items are kept here ([`VaultGrid`]) because the held item and the
//!   server's move answers are shared; an item picked up from the vault and dropped on the inventory
//!   asks to move it back (`SendRequestItemToMyInven`), and the inventory does not close while a move
//!   waits for its answer.
//!
//! Missing: personal store, the window's ancient sets and sockets, potion hotkeys, item stacking
//! (jewels on items, `IsOverlayItem`) and the grid's durability colors.

use bevy::prelude::*;
use bevy::text::{FontSize, FontSmoothing, FontWeight, LineHeight};
use mu_data::items::info::{HeroView, TipLine, item_info};
use mu_data::items::{Item, group, price, slot};

use mu_protocol::game::{ClientMessage, FIRST_GRID_SLOT, ServerMessage, storage};

use crate::Args;
use crate::item_view::{ItemData, ItemView, ItemViewSet, ItemViews};
use crate::scene::GameScene;
use crate::sound::SoundQueue;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Inventory>()
            .add_systems(OnEnter(GameScene::Game), (spawn_root, fill_test_inventory))
            .add_systems(
                OnExit(GameScene::Game),
                (crate::scene::despawn::<InventoryRoot>, |mut inv: ResMut<Inventory>| {
                    // No answer will come for a pending move: the server keeps the item in its slot.
                    inv.pending = false;
                    inv.drop_pending = None;
                    inv.close();
                }),
            )
            .add_systems(PreUpdate, from_server.after(crate::net::NetSystems))
            .add_systems(
                PreUpdate,
                keys.after(bevy::input::InputSystems).after(crate::chat::ChatKeys).run_if(in_state(GameScene::Game)),
            )
            .configure_sets(Update, (InventorySet::Mouse, InventorySet::Draw).chain().before(ItemViewSet))
            .add_systems(
                Update,
                (follow_char_info, update_mouse).chain().in_set(InventorySet::Mouse).run_if(in_state(GameScene::Game)),
            )
            .add_systems(Update, (request_views, draw).chain().in_set(InventorySet::Draw).run_if(in_state(GameScene::Game)));
    }
}

/// The inventory's mouse handling and its drawing; windows that draw with it (the NPC shop) go in
/// between.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum InventorySet {
    Mouse,
    Draw,
}

/// Grid cell of an inventory slot (12-75: `MAX_EQUIPMENT_INDEX` + y · 8 + x).
fn grid_cell(slot: u8) -> Option<(u8, u8)> {
    let i = slot.checked_sub(FIRST_GRID_SLOT)? as usize;
    (i < COLUMNS * ROWS).then_some(((i % COLUMNS) as u8, (i / COLUMNS) as u8))
}

/// Inventory slot of a grid cell.
pub(crate) fn grid_slot(x: u8, y: u8) -> u8 {
    FIRST_GRID_SLOT + y * COLUMNS as u8 + x
}

/// Slot the held item came from: in the inventory `MAX_EQUIPMENT_INDEX + GetSourceLinealPos()` or the
/// equipment `lineal_pos`; in the vault its `GetSourceLinealPos()` (`y · 8 + x`).
pub(crate) fn source_slot(from: Source) -> u8 {
    match from {
        Source::Grid { x, y } => grid_slot(x, y),
        Source::Equipment(s) => s,
        Source::Vault { x, y } => y * VAULT_COLUMNS as u8 + x,
    }
}

/// Storage the held item came from (`REQUEST_EQUIPMENT_INVENTORY` or `_STORAGE`).
pub(crate) fn source_storage(from: Source) -> u8 {
    match from {
        Source::Grid { .. } | Source::Equipment(_) => storage::INVENTORY,
        Source::Vault { .. } => storage::VAULT,
    }
}

/// Inventory packets from the server.
fn from_server(mut from: MessageReader<crate::net::FromServer>, mut inv: ResMut<Inventory>, mut sound: ResMut<SoundQueue>) {
    for m in from.read() {
        if let Some(s) = inv.apply(&m.0) {
            sound.play(s);
        }
    }
}

/// With the character window open, to its left (`640-190*2`); otherwise at `640-190`.
fn follow_char_info(char_info: Res<crate::char_info::CharInfoWindow>, mut inv: ResMut<Inventory>) {
    let dx = if char_info.is_open() { -190.0 } else { 0.0 };
    if inv.dx != dx {
        inv.dx = dx;
    }
}

// --- Measurements (640×480 screen) -------------------------------------------------------------

/// `g_hFont` (like options.rs).
const FONT: f32 = crate::chat::FONT;
/// `m_Pos` (`Create(m_pNewUIMng, m_pNewUI3DRenderMng, 640-190, 0)`).
const POS: (f32, f32) = (640.0 - 190.0, 0.0);
/// `INVENTORY_WIDTH` × `INVENTORY_HEIGHT`.
const W: f32 = 190.0;
const H: f32 = 429.0;
/// The grid: `Create(..., x+15, y+200, 8, 8)` with cells of `INVENTORY_SQUARE_WIDTH/HEIGHT` = 20.
pub const COLUMNS: usize = 8;
pub const ROWS: usize = 8;
const GRID: (f32, f32) = (POS.0 + 15.0, POS.1 + 200.0);
pub(crate) const SQUARE: f32 = 20.0;
/// `WND_TOP_EDGE`, `WND_LEFT_EDGE`, `WND_BOTTOM_EDGE`, `WND_RIGHT_EDGE` (NewUIInventoryCtrl.h:109-112).
const EDGE_TOP: f32 = 3.0;
const EDGE_LEFT: f32 = 4.0;
const EDGE_BOTTOM: f32 = 8.0;
const EDGE_RIGHT: f32 = 9.0;
/// Buttons (`SetButtonInfo`): close, repair and personal store, 36×29.
const BTN_EXIT: (f32, f32) = (POS.0 + 13.0, POS.1 + 391.0);
const BTN_REPAIR: (f32, f32) = (POS.0 + 50.0, POS.1 + 391.0);
const BTN_SHOP: (f32, f32) = (POS.0 + 87.0, POS.1 + 391.0);
pub(crate) const BTN_W: f32 = 36.0;
pub(crate) const BTN_H: f32 = 29.0;
/// `GlobalText[223, 225, 233, 1125, 989, 2651]` (233 "Repair (L)": MuMain Localization/Game.en.resx:649).
const TITLE: &str = "Inventory";
const TIP_EXIT: &str = "Close (I,V)";
pub(crate) const TIP_REPAIR: &str = "Repair (L)";
const TIP_SHOP: &str = "Open personal store(S)";
const SET_OPTION: &str = "[Set option]";
const SOCKET_OPTION: &str = "[Socket option]";

/// `SetEquipmentSlotInfo` (NewUIMyInventory.cpp:1062-1135): (x, y, width, height) relative to `m_Pos`
/// and its background image, per slot (`EQUIPMENT_*`).
const SLOTS: [(f32, f32, f32, f32, Tex); slot::COUNT] = [
    (15.0, 87.0, 46.0, 66.0, Tex::SlotWeaponR),
    (135.0, 87.0, 46.0, 66.0, Tex::SlotWeaponL),
    (75.0, 44.0, 46.0, 46.0, Tex::SlotHelm),
    (75.0, 87.0, 46.0, 66.0, Tex::SlotArmor),
    (75.0, 150.0, 46.0, 46.0, Tex::SlotPants),
    (15.0, 150.0, 46.0, 46.0, Tex::SlotGloves),
    (135.0, 150.0, 46.0, 46.0, Tex::SlotBoots),
    (120.0, 44.0, 61.0, 46.0, Tex::SlotWing),
    (15.0, 44.0, 46.0, 46.0, Tex::SlotFairy),
    (54.0, 87.0, 28.0, 28.0, Tex::SlotNecklace),
    (54.0, 150.0, 28.0, 28.0, Tex::SlotRing),
    (114.0, 150.0, 28.0, 28.0, Tex::SlotRing),
];

fn slot_rect(i: usize) -> (f32, f32, f32, f32) {
    let (x, y, w, h, _) = SLOTS[i];
    (POS.0 + x, POS.1 + y, w, h)
}

/// `CheckMouseIn(x, y, width, height)`.
pub(crate) fn inside(m: Vec2, x: f32, y: f32, w: f32, h: f32) -> bool {
    m.x >= x && m.x < x + w && m.y >= y && m.y < y + h
}

// --- State -------------------------------------------------------------------------------------

/// A grid item, with its top-left cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridItem {
    pub x: u8,
    pub y: u8,
    pub item: Item,
}

/// Where the held item came from (`GetOwnerInventory` or `ex_src_type == ITEM_EX_SRC_EQUIPMENT`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Grid { x: u8, y: u8 },
    Equipment(u8),
    /// The vault's grid (`g_pStorageInventory->GetInventoryCtrl()`).
    Vault { x: u8, y: u8 },
}

/// The vault grid: `CNewUIStorageInventory`'s `Create(..., 8, 15)` (NewUIStorageInventory.cpp:42-43).
pub const VAULT_COLUMNS: usize = 8;
pub const VAULT_ROWS: usize = 15;

/// The vault's items (`CNewUIStorageInventory::m_pNewInventoryCtrl`) and what the moves between it and
/// the inventory need. They live here because the held item (`CNewUIPickedItem`) and the server's move
/// answers (`ReceiveEquipmentItem`) are shared by both windows; vault.rs runs the window itself.
#[derive(Default, Debug)]
pub struct VaultGrid {
    /// `IsVisible(INTERFACE_STORAGE)`.
    pub open: bool,
    /// By top-left cell.
    pub items: Vec<GridItem>,
    /// `m_bItemAutoMove` with the cell of the item it moves (the original keeps the mouse position,
    /// `m_nBackupMouseX/Y`, and looks the item up there): a right click sent the item to the other
    /// window and it leaves its own grid when the server says where it went.
    pub(crate) auto_move: Option<Source>,
}

impl VaultGrid {
    /// The vault item covering a cell.
    pub(crate) fn owner(&self, data: &ItemData, x: usize, y: usize) -> Option<usize> {
        owner_in(&self.items, data, x, y)
    }

    /// `CanMove` / `CheckSlot` in the vault grid.
    pub(crate) fn fits(&self, data: &ItemData, x: i32, y: i32, w: u8, h: u8) -> bool {
        fits_in(&self.items, data, VAULT_COLUMNS, VAULT_ROWS, x, y, w, h)
    }

    /// `CNewUIStorageInventory::FindEmptySlot` (NewUIStorageInventory.cpp:517-528): a vault slot.
    pub(crate) fn find_empty(&self, data: &ItemData, item: &Item) -> Option<u8> {
        let (w, h) = Inventory::size(data, item);
        find_empty_in(&self.items, data, VAULT_COLUMNS, VAULT_ROWS, w, h).map(|(x, y)| y * VAULT_COLUMNS as u8 + x)
    }

    /// `InsertItem(index, packet)` (`AddItem`: the cell is `index % 8, index / 8`); slots past the
    /// 8×15 grid (OpenMU's extended vault) are dropped.
    pub(crate) fn insert(&mut self, slot: u8, item: Item) {
        let i = slot as usize;
        if i >= VAULT_COLUMNS * VAULT_ROWS {
            return;
        }
        let (x, y) = ((i % VAULT_COLUMNS) as u8, (i / VAULT_COLUMNS) as u8);
        self.items.retain(|g| (g.x, g.y) != (x, y));
        self.items.push(GridItem { x, y, item });
    }
}

/// The grid item covering a cell (`m_pdwItemCheckBox`).
fn owner_in(items: &[GridItem], data: &ItemData, x: usize, y: usize) -> Option<usize> {
    items.iter().position(|g| {
        let (w, h) = Inventory::size(data, &g.item);
        (g.x as usize..g.x as usize + w as usize).contains(&x) && (g.y as usize..g.y as usize + h as usize).contains(&y)
    })
}

/// `CheckSlot`: whether a `w`×`h` item fits at (x, y) of a `cols`×`rows` grid. (The original counts
/// consecutive cells and, past the right edge, wraps to the next row; the server rejects that. Here
/// it must stay inside.)
#[allow(clippy::too_many_arguments)]
fn fits_in(items: &[GridItem], data: &ItemData, cols: usize, rows: usize, x: i32, y: i32, w: u8, h: u8) -> bool {
    if x < 0 || y < 0 || x as usize + w as usize > cols || y as usize + h as usize > rows {
        return false;
    }
    (0..h as usize).all(|dy| (0..w as usize).all(|dx| owner_in(items, data, x as usize + dx, y as usize + dy).is_none()))
}

/// `FindEmptySlot`: the first cell (row by row) where a `w`×`h` item fits.
fn find_empty_in(items: &[GridItem], data: &ItemData, cols: usize, rows: usize, w: u8, h: u8) -> Option<(u8, u8)> {
    (0..rows as i32)
        .flat_map(|y| (0..cols as i32).map(move |x| (x, y)))
        .find(|&(x, y)| fits_in(items, data, cols, rows, x, y, w, h))
        .map(|(x, y)| (x as u8, y as u8))
}

/// `CNewUIPickedItem`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Picked {
    pub item: Item,
    pub from: Source,
}

/// Grid `m_EventState`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(crate) enum CtrlEvent {
    #[default]
    None,
    Hover,
    Picking,
}

/// The character's inventory and its window.
#[derive(Resource, Default)]
pub struct Inventory {
    pub open: bool,
    pub grid: Vec<GridItem>,
    pub equipment: [Option<Item>; slot::COUNT],
    /// `CharacterMachine->Gold` without a server (with one, `CharacterInformation`'s is used).
    pub zen: u32,
    pub picked: Option<Picked>,
    /// Bumped whenever the equipment changes (to re-dress the character).
    pub equipment_rev: u32,
    /// `EquipmentItem`: a move was sent and its answer has not arrived yet.
    pending: bool,
    /// `SendDropItem`: the slot whose drop on the ground (ground.rs) waits for `ReceiveDropItem`.
    pub(crate) drop_pending: Option<u8>,
    event: CtrlEvent,
    /// `m_iPointedSquareIndex` and `m_iPointedSlot`.
    pointed_square: Option<usize>,
    pointed_slot: Option<usize>,
    /// `m_pToolTipItem`: index into `grid`.
    tooltip: Option<usize>,
    /// The close button is held down over it (`BUTTON_STATE_DOWN`).
    exit_down: bool,
    /// The repair button is held down (`BUTTON_STATE_DOWN`).
    repair_down: bool,
    /// `m_RepairMode` (`REPAIR_MODE_ON`).
    pub repair_mode: bool,
    /// The original's global `RepairEnable`: 2 after a repair tooltip of an item that needs repairing,
    /// 1 after one that does not (`RenderRepairInfo`, ZzzInventory.cpp:5845 and 5857); 0 when the
    /// inventory closes (`ClosingProcess`, NewUIMyInventory.cpp:821). The cursor reads it.
    pub repair_enable: u8,
    /// X offset from `POS`: with the character window open the original moves it to
    /// `640-190*2` (`SetPos`, NewUISystem.cpp:590-609) and back to `640-190` on close (974).
    /// Everything is computed at `POS` and shifted when reading the mouse and drawing.
    pub dx: f32,
    /// The vault's grid while it is open (vault.rs).
    pub vault: VaultGrid,
}

impl Inventory {
    /// `ItemAttribute.Width/Height` (1×1 if not in the table).
    pub(crate) fn size(data: &ItemData, item: &Item) -> (u8, u8) {
        data.attr(item).map_or((1, 1), |a| (a.width.max(1), a.height.max(1)))
    }

    /// The item occupying the cell (`m_pdwItemCheckBox`).
    fn owner(&self, data: &ItemData, x: usize, y: usize) -> Option<usize> {
        owner_in(&self.grid, data, x, y)
    }

    /// `CheckSlot` in the inventory grid (see [`fits_in`]).
    fn fits(&self, data: &ItemData, x: i32, y: i32, w: u8, h: u8) -> bool {
        fits_in(&self.grid, data, COLUMNS, ROWS, x, y, w, h)
    }

    /// `FindEmptySlot`: the first cell (row by row) where it fits.
    pub fn find_empty(&self, data: &ItemData, item: &Item) -> Option<(u8, u8)> {
        let (w, h) = Self::size(data, item);
        find_empty_in(&self.grid, data, COLUMNS, ROWS, w, h)
    }

    /// `EquipmentItem`: a move was sent and its answer has not arrived yet.
    pub(crate) fn busy(&self) -> bool {
        self.pending
    }

    /// The inventory grid item under the mouse (640×480 screen): `FindItemAtPt` (index into `grid`).
    pub(crate) fn grid_item_at(&self, data: &ItemData, c: Vec2) -> Option<usize> {
        let s = square_at(self.local(c))?;
        self.owner(data, s % COLUMNS, s / COLUMNS)
    }

    /// Puts the item in the first free cell (for the test inventory).
    pub fn add(&mut self, data: &ItemData, item: Item) -> bool {
        match self.find_empty(data, &item) {
            Some((x, y)) => {
                self.grid.push(GridItem { x, y, item });
                true
            }
            None => false,
        }
    }

    /// The inventory sent by the server (`ReceiveInventory`): slot (0-11 equipment, 12-75 grid, the
    /// rest is the personal store, not shown yet) and the item's 12 bytes (`Item::from_bytes`).
    pub fn set_items(&mut self, list: &[(u8, [u8; 12])]) {
        self.grid.clear();
        self.equipment = [None; slot::COUNT];
        self.picked = None;
        self.pending = false;
        self.drop_pending = None;
        self.event = CtrlEvent::None;
        self.tooltip = None;
        for (s, bytes) in list {
            if let Some(item) = Item::from_bytes(bytes) {
                self.place(*s, item);
            }
        }
        self.equipment_rev += 1;
    }

    /// `EquipItem` / `InsertItem`: puts `item` in inventory slot `slot` (0-11 equipment, 12-75 grid by
    /// its top-left cell). Other slots are ignored.
    fn place(&mut self, slot: u8, item: Item) {
        let s = slot as usize;
        if s < slot::COUNT {
            self.equipment[s] = Some(item);
            self.equipment_rev += 1;
        } else if let Some((x, y)) = grid_cell(slot) {
            self.grid.retain(|g| (g.x, g.y) != (x, y));
            self.grid.push(GridItem { x, y, item });
        }
    }

    /// The item in inventory slot `slot` (a grid item by its top-left cell).
    fn item_in(&mut self, slot: u8) -> Option<&mut Item> {
        let s = slot as usize;
        if s < slot::COUNT {
            return self.equipment[s].as_mut();
        }
        let (x, y) = grid_cell(slot)?;
        self.grid.iter_mut().find(|g| (g.x, g.y) == (x, y)).map(|g| &mut g.item)
    }

    /// `ReceiveDeleteInventory` (`UnequipItem` / `DeleteItem`).
    fn remove(&mut self, slot: u8) {
        let s = slot as usize;
        if s < slot::COUNT {
            if self.equipment[s].take().is_some() {
                self.equipment_rev += 1;
            }
        } else if let Some((x, y)) = grid_cell(slot) {
            self.grid.retain(|g| (g.x, g.y) != (x, y));
        }
        self.tooltip = None;
    }

    /// Applies what the server says about the inventory. Returns the sound to play, if any.
    pub fn apply(&mut self, msg: &ServerMessage) -> Option<&'static str> {
        match msg {
            ServerMessage::InventoryList(list) => self.set_items(list),
            ServerMessage::ItemMoved { storage: to, slot, item } => {
                // `ReceiveEquipmentItem` (WSclient.cpp:5735-5764): `DeletePickedItem` and then
                // `EquipItem` / `InsertItem` at the server's slot.
                self.pending = false;
                self.picked = None;
                self.event = CtrlEvent::None;
                let item = Item::from_bytes(item);
                match *to {
                    storage::INVENTORY => {
                        // A right click from the vault: the item leaves the vault grid
                        // (`ProcessStorageItemAutoMoveSuccess`, only for a grid slot).
                        if grid_cell(*slot).is_some()
                            && let Some(Source::Vault { x, y }) = self.vault.auto_move
                        {
                            self.vault.auto_move = None;
                            self.vault.items.retain(|g| (g.x, g.y) != (x, y));
                        }
                        if let Some(item) = item {
                            self.place(*slot, item);
                        }
                    }
                    // `ProcessToReceiveStorageItems` (NewUIStorageInventory.cpp:598-616): an item
                    // moved by a right click leaves the inventory grid, then the vault shows it.
                    storage::VAULT if (*slot as usize) < VAULT_COLUMNS * VAULT_ROWS => {
                        if let Some(Source::Grid { x, y }) = self.vault.auto_move.take() {
                            self.grid.retain(|g| (g.x, g.y) != (x, y));
                            self.tooltip = None;
                        }
                        if let Some(item) = item {
                            self.vault.insert(*slot, item);
                        }
                    }
                    _ => {}
                }
                return Some("SOUND_GET_ITEM01");
            }
            ServerMessage::ItemMoveFailed => {
                // `BackupPickedItem` and `ProcessStorageItemAutoMoveFailure`.
                self.pending = false;
                self.backup_picked();
                self.vault.auto_move = None;
            }
            ServerMessage::ItemBought { slot, item } => {
                // `ReceiveBuy` (WSclient.cpp:6003-6030): `InsertItem` for a grid slot and
                // `SOUND_GET_ITEM01` (the shop clears its pending purchase, shop.rs).
                if grid_cell(*slot).is_some()
                    && let Some(item) = Item::from_bytes(item)
                {
                    self.place(*slot, item);
                }
                return Some("SOUND_GET_ITEM01");
            }
            ServerMessage::SellResult { result, .. } => match result {
                // `ReceiveSell` (WSclient.cpp:6032-6064): refused, the item goes back (with 0xFE
                // `HideAll` closes the inventory, which puts it back too).
                0 | 0xFF => self.backup_picked(),
                0xFE => {}
                // Sold: `DeletePickedItem` (the zen is set by shop.rs).
                _ => {
                    if self.picked.take().is_some() {
                        self.event = CtrlEvent::None;
                    }
                    return Some("SOUND_GET_ITEM01");
                }
            },
            ServerMessage::ItemRemoved { slot } => self.remove(*slot),
            ServerMessage::ItemDropResult { success, slot } => {
                // `ReceiveDropItem` (WSclient.cpp:5663-5685): dropped, `UnequipItem` / `DeleteItem` of
                // the slot (it is there if the item went back meanwhile, e.g. the window closed) and
                // `DeletePickedItem`; refused, `BackupPickedItem`. `SendDropItem = -1` either way.
                self.drop_pending = None;
                if *success {
                    self.remove(*slot);
                    // Only the dropped item (the original deletes whatever is held, even another one
                    // picked up after this one went back).
                    if self.picked.is_some_and(|p| source_storage(p.from) == storage::INVENTORY && source_slot(p.from) == *slot) {
                        self.picked = None;
                        self.event = CtrlEvent::None;
                    }
                } else if self.picked.is_some_and(|p| source_storage(p.from) == storage::INVENTORY && source_slot(p.from) == *slot) {
                    self.backup_picked();
                }
            }
            // `ReceiveGetItem` (WSclient.cpp:5629-5633): a picked up item goes into its grid slot
            // (`InsertItem`); the message and the sound are ground.rs'.
            ServerMessage::ItemPickedUp { slot, item } => {
                if grid_cell(*slot).is_some()
                    && let Some(item) = Item::from_bytes(item)
                {
                    self.place(*slot, item);
                }
            }
            ServerMessage::ItemDurability { slot, durability, .. } => {
                if let Some(item) = self.item_in(*slot) {
                    item.durability = *durability;
                }
            }
            _ => {}
        }
        None
    }

    /// `SendRequestEquipmentItem(REQUEST_EQUIPMENT_INVENTORY, source, item, REQUEST_EQUIPMENT_INVENTORY,
    /// target)`: asks to move the held item to inventory slot `to`. Ignored while another move is
    /// pending (`if(EquipmentItem ...) return false`).
    /// The held item goes to `to_storage`/`to_slot` from where it was picked up (its storage and
    /// slot, see [`source_storage`]).
    pub(crate) fn request_move_to(&mut self, net: &mut crate::net::Net, to_storage: u8, to_slot: u8) -> bool {
        let Some(p) = self.picked else { return false };
        self.send_move(net, source_storage(p.from), source_slot(p.from), p.item, to_storage, to_slot)
    }

    /// `SendRequestEquipmentItem` (wsclientinline.h:950-954): nothing while another move waits for its
    /// answer (`EquipmentItem`). Returns whether it was sent.
    pub(crate) fn send_move(&mut self, net: &mut crate::net::Net, from_storage: u8, from_slot: u8, item: Item, to_storage: u8, to_slot: u8) -> bool {
        // Also not while the held item's drop on the ground waits for its answer: the original allows
        // it (`SendDropItem` is not checked), but then both answers would act on the same item.
        if self.pending || self.drop_pending.is_some() {
            return false;
        }
        net.send(ClientMessage::ItemMove { from_storage, from_slot, item: item.to_bytes(), to_storage, to_slot });
        self.pending = true;
        true
    }

    /// Dropping the held item on inventory slot `to`: from the vault through
    /// `CNewUIStorageInventory::SendRequestItemToMyInven` (NewUIStorageInventory.cpp:407-424), which
    /// asks for the vault pin first when the vault is locked (`needs_pin`).
    fn request_to_inventory(&mut self, net: &mut crate::net::Net, to: u8, needs_pin: bool) {
        let Some(p) = self.picked else { return };
        if matches!(p.from, Source::Vault { .. }) && needs_pin {
            // TODO: `HidePickedItem` and the `CPasswordKeyPadMsgBoxLayout` keypad; without it this is
            // the keypad's Cancel (`ShowPickedItem`): the item stays on the mouse.
            warn!("baúl: está cerrado con clave; falta el teclado numérico para sacar objetos");
            return;
        }
        self.request_move_to(net, storage::INVENTORY, to);
    }

    /// `BackupPickedItem`: the held item returns to its place. Not while a move waits for its answer
    /// (`if(ms_pPickedItem && EquipmentItem == false)`, NewUIInventoryCtrl.cpp:1481).
    pub(crate) fn backup_picked(&mut self) {
        if self.pending {
            return;
        }
        let Some(p) = self.picked.take() else { return };
        match p.from {
            Source::Grid { x, y } => self.grid.push(GridItem { x, y, item: p.item }),
            Source::Equipment(s) => {
                self.equipment[s as usize] = Some(p.item);
                self.equipment_rev += 1;
            }
            Source::Vault { x, y } => self.vault.items.push(GridItem { x, y, item: p.item }),
        }
        self.event = CtrlEvent::None;
    }

    /// `ClosingProcess` (NewUIMyInventory.cpp:818-823): the held item goes back, `RepairEnable = 0`
    /// and repair mode off. With the vault open, `Hide(INTERFACE_INVENTORY)` stops first if the vault's
    /// `ProcessClosing` refuses (a move waiting for its answer, NewUISystem.cpp:1017-1022 and
    /// NewUIStorageInventory.cpp:273-276): nothing closes.
    fn close(&mut self) {
        if self.vault.open && self.pending {
            return;
        }
        self.backup_picked();
        self.open = false;
        self.tooltip = None;
        self.exit_down = false;
        self.repair_down = false;
        self.repair_enable = 0;
        self.set_repair_mode(false);
    }

    /// `SetRepairMode` (NewUIMyInventory.cpp:427-445): also the grid's (`CNewUIInventoryCtrl::
    /// SetRepairMode`, which switches its tooltips to `TOOLTIP_TYPE_REPAIR`).
    pub fn set_repair_mode(&mut self, on: bool) {
        self.repair_mode = on;
        self.tooltip = None;
        self.event = CtrlEvent::None;
    }

    /// `g_pNewUISystem->Toggle(INTERFACE_INVENTORY)`.
    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            // `OpenningProcess` (NewUIMyInventory.cpp:781-816): repair mode off.
            self.open = true;
            self.set_repair_mode(false);
        }
    }

    /// `m_pNewUIMng->ShowInterface(INTERFACE_INVENTORY)`: shown by a window that comes with it (the
    /// NPC shop, NewUISystem.cpp:659-665), without the inventory's `OpenningProcess`.
    pub fn show(&mut self) {
        if !self.open {
            self.open = true;
            self.event = CtrlEvent::None;
        }
    }

    /// Asks the server to repair inventory slot `slot` (`SendRequestRepair(index, addGold)`):
    /// `self_repair` when paying from the inventory (`p_AddGold` 1). Durability and zen change when
    /// the server answers (`ItemDurability`, `InventoryMoneyUpdate`).
    pub(crate) fn request_repair(net: &mut crate::net::Net, slot: u8, self_repair: bool) {
        if net.connected() {
            net.send(ClientMessage::RepairItem { slot, self_repair });
        }
    }

    /// The mouse (640×480 screen) is over the window (`CheckMouseIn(m_Pos, 190, 429)`), or an item is
    /// held (a click in the game would drop it): no walking.
    pub fn blocks(&self, c: Vec2) -> bool {
        let l = self.local(c);
        self.open && (inside(l, POS.0, POS.1, W, H) || (self.picked.is_some() && c.y < 429.0))
    }

    /// From the on-screen mouse to window coordinates at `POS`.
    fn local(&self, c: Vec2) -> Vec2 {
        Vec2::new(c.x - self.dx, c.y)
    }
}

/// `IsEquipable` (NewUIMyInventory.cpp:199-362) without pets or special maps. Without the
/// character data, strength, agility... are not checked.
fn is_equipable(inv: &Inventory, data: &ItemData, hero: &HeroView, slot_index: usize, item: &Item) -> bool {
    let Some(attr) = data.attr(item) else { return false };
    let class = hero.class as usize;
    let rc = attr.require_class;
    // The Magic Gladiator wears what both Dark Wizard and Dark Knight can.
    let mut ok = rc[class] != 0 || (class == 3 && rc[0] != 0 && rc[1] != 0);
    if rc[class] > hero.step.max(1) {
        return false;
    }
    if !ok {
        return false;
    }
    let s = slot_index as u8;
    ok = attr.slot == Some(s);
    if attr.slot == Some(slot::WEAPON_RIGHT) && s == slot::WEAPON_LEFT {
        // Dark Knight, Magic Gladiator and Rage Fighter: one-handed weapons on the left; the Summoner,
        // anything but a staff.
        if matches!(class, 1 | 3 | 6) {
            ok = !attr.two_hand;
        } else if class == 5 && item.group != group::STAFF {
            ok = true;
        }
    } else if attr.slot == Some(slot::RING_RIGHT) && s == slot::RING_LEFT {
        ok = true;
    }
    // An Elf with a bow on the left can only hold crossbow bolts (`ITEM_BOW+15`) on the right.
    if class == 2 && s == slot::WEAPON_RIGHT {
        if let Some(l) = inv.equipment[slot::WEAPON_LEFT as usize] {
            if l.group == group::BOW && l.index != 7 && item.kind() != mu_data::items::kind(group::BOW, 15) {
                ok = false;
            }
        }
    }
    // The Rage Fighter does not wear gloves.
    if class == 6 && s == slot::GLOVES {
        ok = false;
    }
    if !ok {
        return false;
    }
    let stats = mu_data::items::convert(attr, item, hero.level);
    if stats.require_level > hero.level {
        return false;
    }
    if let Some([str_, agi, vit, ene, cha]) = hero.stats {
        if stats.require_strength > str_
            || stats.require_dexterity > agi
            || stats.require_vitality > vit
            || stats.require_energy > ene
            || stats.require_charisma > cha
        {
            return false;
        }
    }
    true
}

/// The character viewing the inventory (`Hero->Class` and `CharacterAttribute`).
pub(crate) fn hero_view(hero: Option<&crate::account::Hero>, info: Option<&crate::account::HeroInfo>) -> HeroView {
    HeroView {
        class: hero.map_or(1, |h| h.class.index() as u8),
        // Class evolution is not known yet: always the first step.
        step: 1,
        level: hero.map_or(1, |h| h.level),
        stats: info.map(|i| {
            let i = &i.0;
            [i.strength as u32, i.agility as u32, i.vitality as u32, i.energy as u32, i.leadership as u32]
        }),
    }
}

/// `--inventario-prueba`: an inventory for inspecting items and their glow.
fn fill_test_inventory(args: Res<Args>, data: Res<ItemData>, mut inv: ResMut<Inventory>) {
    if !args.test_inventory || !inv.grid.is_empty() || inv.equipment.iter().any(|e| e.is_some()) {
        return;
    }
    let it = |g: u8, i: u16, level: u8| Item { level, durability: 255, ..Item::new(g, i) };
    // Dark Knight gear: +9 sword with luck and skill, +7 shield, +11 to +15 dragon set
    // (excellent armor), Satan wings, ring and pendant.
    let mut equip = [None; slot::COUNT];
    equip[slot::WEAPON_RIGHT as usize] = Some(Item { skill: true, luck: true, option: 3, ..it(group::SWORD, 5, 9) });
    equip[slot::WEAPON_LEFT as usize] = Some(it(group::SHIELD, 4, 7));
    equip[slot::HELM as usize] = Some(it(group::HELM, 1, 11));
    equip[slot::ARMOR as usize] = Some(Item { excellent: 0b10_0001, ..it(group::ARMOR, 1, 13) });
    equip[slot::PANTS as usize] = Some(it(group::PANTS, 1, 15));
    equip[slot::GLOVES as usize] = Some(it(group::GLOVES, 1, 9));
    equip[slot::BOOTS as usize] = Some(it(group::BOOTS, 1, 7));
    equip[slot::WING as usize] = Some(it(group::WING, 2, 0));
    equip[slot::AMULET as usize] = Some(it(group::HELPER, 12, 0));
    equip[slot::RING_RIGHT as usize] = Some(Item { option: 2, ..it(group::HELPER, 8, 0) });
    inv.equipment = equip;
    inv.equipment_rev += 1;
    let grid = [
        it(group::SWORD, 0, 0),
        it(group::SWORD, 1, 3),
        Item { excellent: 0b01_0100, ..it(group::SWORD, 3, 5) },
        it(group::AXE, 3, 7),
        it(group::MACE, 2, 9),
        it(group::SPEAR, 4, 11),
        Item { excellent: 0b11_0000, luck: true, ..it(group::BOW, 10, 13) },
        Item { excellent: 0b00_1001, ..it(group::STAFF, 2, 15) },
        // Ancient (`ExtOption % 4` = 1).
        Item { ancient: 5, ..it(group::ARMOR, 5, 0) },
        it(group::HELM, 0, 13),
        it(group::SHIELD, 2, 15),
        it(group::POTION, 13, 0),
        it(group::POTION, 14, 0),
        it(group::WING, 15, 0),
        Item { durability: 3, ..it(group::POTION, 3, 0) },
        Item { excellent: 0b00_0010, ..it(group::HELPER, 9, 0) },
    ];
    for item in grid {
        if !inv.add(&data, item) {
            warn!("inventario de prueba: no cabe {},{}", item.group, item.index);
        }
    }
    inv.zen = 2_500_000;
    // Opens by itself so it is visible right away.
    inv.open = true;
    info!("inventario de prueba: {} objetos en la rejilla", inv.grid.len());
}

// --- Keyboard and mouse ------------------------------------------------------------------------

/// `I`/`V` open or close (`CNewUIHotKey`; blocked while message boxes are up); Esc closes (before the
/// system menu sees it: the window comes first in the original's key order).
#[allow(clippy::too_many_arguments)]
fn keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    game_ui: Res<crate::options::GameUi>,
    move_window: Res<crate::move_window::MoveWindow>,
    shop: Res<crate::shop::NpcShop>,
    dialog: Res<crate::npc::NpcDialog>,
    hero: Option<Res<crate::account::Hero>>,
    mut inv: ResMut<Inventory>,
    mut sound: ResMut<SoundQueue>,
) {
    if game_ui.modal() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyI) || keys.just_pressed(KeyCode::KeyV) {
        // Not while a sale waits for the server (NewUIHotKey.cpp:157-165 and 198-206).
        if !shop.selling {
            inv.toggle();
            sound.play("SOUND_CLICK01");
        }
    } else if inv.open
        && keys.just_pressed(KeyCode::Escape)
        && !game_ui.option_open()
        && !game_ui.window_menu_open()
        && !move_window.is_open()
    {
        // NewUIMyInventory.cpp:564-572: Esc is taken even while selling, but closes nothing then.
        if !shop.selling {
            inv.close();
            sound.play("SOUND_CLICK01");
        }
        keys.clear_just_pressed(KeyCode::Escape);
    } else if inv.open
        && keys.just_pressed(KeyCode::KeyL)
        && repair_enable_level(hero.as_deref())
        && !matches!(dialog.kind(), Some(crate::npc::WindowKind::Shop | crate::npc::WindowKind::ChaosMachine))
    {
        // `ToggleRepairMode` (NewUIMyInventory.cpp:573-586).
        let on = !inv.repair_mode;
        inv.set_repair_mode(on);
    }
}

/// `m_bRepairEnableLevel`: level 50 or more (`OpenningProcess`, NewUIMyInventory.cpp:788-797).
fn repair_enable_level(hero: Option<&crate::account::Hero>) -> bool {
    hero.is_some_and(|h| h.level >= 50)
}

/// `RenderButtons` / `BtnProcess` (NewUIMyInventory.cpp:1305-1313 and 2197-2205): the repair and
/// personal store buttons only work without the NPC shop, vault, Chaos Machine, Devil Square or
/// Blood Castle windows (no trade window yet).
fn own_buttons(dialog: &crate::npc::NpcDialog) -> bool {
    use crate::npc::WindowKind::*;
    !matches!(dialog.kind(), Some(Shop | Vault | ChaosMachine | DevilSquare | BloodCastle))
}

/// `CNewUIMyInventory::Update` + `UpdateMouseEvent` (NewUIMyInventory.cpp:447-558 and 653-675).
#[allow(clippy::too_many_arguments)]
fn update_mouse(
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    data: Res<ItemData>,
    game_ui: Res<crate::options::GameUi>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    (shop, dialog, vault): (Res<crate::shop::NpcShop>, Res<crate::npc::NpcDialog>, Res<crate::vault::Vault>),
    mut inv: ResMut<Inventory>,
    mut sound: ResMut<SoundQueue>,
    mut net: ResMut<crate::net::Net>,
) {
    if !inv.open {
        return;
    }
    let online = net.connected();
    let needs_pin = vault.needs_pin();
    let Some(m) = windows.single().ok().and_then(crate::mainframe::cursor_virtual).filter(|_| !game_ui.modal()) else {
        return;
    };
    // `MouseX`, `MouseY`: integers on the 640×480 screen.
    let m = inv.local(m.floor());
    let repair_level = repair_enable_level(hero.as_deref());
    let hero = hero_view(hero.as_deref(), info.as_deref());
    let (pressed, released) = (mouse.pressed(MouseButton::Left), mouse.just_released(MouseButton::Left));
    let just_pressed = mouse.just_pressed(MouseButton::Left);
    // Who pays a repair (NewUIMyInventory.cpp:1477-1484 and 2163-2170): the repair shop, or the
    // inventory's own repair from level 50.
    let repair_by = if shop.open && shop.repair_shop {
        Some(false)
    } else if repair_level {
        Some(true)
    } else {
        None
    };
    let inv = &mut *inv;

    // `Update`: the equipment slot under the mouse (`CheckMouseIn(x + 1, y, w - 4, h - 4)`).
    inv.pointed_slot = (0..slot::COUNT).find(|&i| {
        let (x, y, w, h) = slot_rect(i);
        inside(m, x + 1.0, y, w - 4.0, h - 4.0)
    });
    // `CNewUIInventoryCtrl::UpdateProcess`.
    let square = square_at(m);
    if square != inv.pointed_square {
        inv.pointed_square = square;
    }
    let pointed_item = inv.pointed_square.and_then(|s| inv.owner(&data, s % COLUMNS, s / COLUMNS));
    if pointed_item.is_none() {
        inv.event = CtrlEvent::None;
        inv.tooltip = None;
    }

    // `CNewUIInventoryCtrl::UpdateMouseEvent`: no picking up in repair mode (NewUIInventoryCtrl.cpp:798).
    if inv.event == CtrlEvent::None && !pressed && pointed_item.is_some() {
        inv.event = CtrlEvent::Hover;
    } else if inv.event == CtrlEvent::Hover && released && inv.picked.is_none() && !inv.repair_mode {
        if let Some(i) = pointed_item {
            inv.event = CtrlEvent::Picking;
            let g = inv.grid.remove(i);
            inv.picked = Some(Picked { item: g.item, from: Source::Grid { x: g.x, y: g.y } });
            inv.tooltip = None;
            return;
        }
    } else if inv.event == CtrlEvent::Hover && !pressed && inv.picked.is_none() {
        if pointed_item != inv.tooltip {
            inv.tooltip = pointed_item;
        }
    }
    // `RenderRepairInfo` leaves `RepairEnable` at 2 or 1 for the item it shows (ZzzInventory.cpp:5845-5857).
    if inv.repair_mode {
        let shown = inv.tooltip.and_then(|i| inv.grid.get(i)).map(|g| g.item).or_else(|| inv.pointed_slot.and_then(|s| inv.equipment[s]));
        if let Some(item) = shown
            && let Some(attr) = data.attr(&item)
            && !price::is_repair_ban(&item)
        {
            inv.repair_enable = if (item.durability as u32) < mu_data::items::max_durability(attr, &item) { 2 } else { 1 };
        }
    }

    // `EquipmentWindowProcess`.
    if let (Some(s), true) = (inv.pointed_slot, released) {
        match inv.picked {
            Some(p) => {
                if inv.equipment[s].is_none() && is_equipable(inv, &data, &hero, s, &p.item) {
                    match p.from {
                        // Same slot: put it back (`BackupPickedItem`).
                        Source::Equipment(from) if from as usize == s => inv.backup_picked(),
                        _ if online => inv.request_to_inventory(&mut net, s as u8, needs_pin),
                        // No server (`--inventario-prueba`): what its reply would do.
                        _ => {
                            inv.equipment[s] = Some(p.item);
                            inv.picked = None;
                            inv.equipment_rev += 1;
                            inv.event = CtrlEvent::None;
                            sound.play("SOUND_GET_ITEM01");
                        }
                    }
                }
            }
            // Repair mode (1463-1487): the equipped item is repaired instead of picked up.
            None if inv.repair_mode => {
                if let (Some(item), Some(self_repair)) = (inv.equipment[s], repair_by)
                    && !price::is_repair_ban(&item)
                {
                    Inventory::request_repair(&mut net, s as u8, self_repair);
                }
            }
            None => {
                if let Some(item) = inv.equipment[s].take() {
                    inv.picked = Some(Picked { item, from: Source::Equipment(s as u8) });
                    inv.equipment_rev += 1;
                }
            }
        }
        return;
    }

    // `InventoryProcess`: drop onto the grid.
    if inside(m, POS.0, POS.1, W, H)
        && released
        && let Some(p) = inv.picked
    {
        let (w, h) = Inventory::size(&data, &p.item);
        // `GetTargetPos`: the mouse minus half the item (`(Width - 1) · 20 / 2`).
        let tx = m.x - ((w as i32 - 1) * SQUARE as i32 / 2) as f32;
        let ty = m.y - ((h as i32 - 1) * SQUARE as i32 / 2) as f32;
        if let Some(t) = square_at(Vec2::new(tx, ty)) {
            let (x, y) = ((t % COLUMNS) as u8, (t / COLUMNS) as u8);
            if inv.fits(&data, x as i32, y as i32, w, h) {
                if p.from == (Source::Grid { x, y }) {
                    inv.backup_picked();
                } else if online {
                    inv.request_to_inventory(&mut net, grid_slot(x, y), needs_pin);
                } else {
                    inv.grid.push(GridItem { x, y, item: p.item });
                    inv.picked = None;
                    inv.event = CtrlEvent::None;
                    sound.play("SOUND_GET_ITEM01");
                }
                return;
            }
        }
    } else if inside(m, POS.0, POS.1, W, H) && inv.repair_mode && just_pressed {
        // Repair mode, left press on a grid item (2148-2173): `MAX_EQUIPMENT + x + y · 8`.
        if let Some(i) = pointed_item {
            let g = inv.grid[i];
            if let Some(self_repair) = repair_by
                && !price::is_repair_ban(&g.item)
            {
                Inventory::request_repair(&mut net, grid_slot(g.x, g.y), self_repair);
            }
            return;
        }
    }

    // `BtnProcess`: the top X (`m_Pos + (169, 7)`, 13×12), the close button (released over it) and the
    // repair button (2207-2211).
    let over_exit = inside(m, BTN_EXIT.0, BTN_EXIT.1, BTN_W, BTN_H);
    let over_repair = repair_level && own_buttons(&dialog) && inside(m, BTN_REPAIR.0, BTN_REPAIR.1, BTN_W, BTN_H);
    if just_pressed && inside(m, POS.0 + 169.0, POS.1 + 7.0, 13.0, 12.0) {
        inv.close();
        return;
    }
    if just_pressed {
        inv.exit_down = over_exit;
        inv.repair_down = over_repair;
    }
    if !pressed {
        if inv.exit_down && over_exit && released {
            inv.close();
            sound.play("SOUND_CLICK01");
            return;
        }
        if inv.repair_down && over_repair && released {
            // `ToggleRepairMode` (2368-2379).
            let on = !inv.repair_mode;
            inv.set_repair_mode(on);
        }
        inv.exit_down = false;
        inv.repair_down = false;
    }
}

/// `GetSquareIndexAtPt`.
fn square_at(p: Vec2) -> Option<usize> {
    if !inside(p, GRID.0, GRID.1, COLUMNS as f32 * SQUARE, ROWS as f32 * SQUARE) {
        return None;
    }
    let (x, y) = (((p.x - GRID.0) / SQUARE) as usize, ((p.y - GRID.1) / SQUARE) as usize);
    Some(y * COLUMNS + x)
}

/// What the item camera draws: `CNewUIMyInventory::Render3D` (equipment, armor 10 higher),
/// `CNewUIInventoryCtrl::Render3D` (the grid) and `CNewUIPickedItem::Render3D` (the held item,
/// centered on the mouse).
fn request_views(
    windows: Query<&Window>,
    data: Res<ItemData>,
    opts: Res<crate::options::GameOptions>,
    inv: Res<Inventory>,
    (shop, vault): (Res<crate::shop::ShopDraw>, Res<crate::vault::VaultDraw>),
    mut views: ResMut<ItemViews>,
) {
    let mut out = Vec::new();
    if inv.open {
        let m = windows.single().ok().and_then(crate::mainframe::cursor_virtual).map(|c| inv.local(c.floor()));
        let hovered = |r: [f32; 4]| m.is_some_and(|m| inside(m, r[0], r[1], r[2], r[3]));
        let free = inv.picked.is_none();
        for (s, item) in inv.equipment.iter().enumerate() {
            let Some(item) = item else { continue };
            let (x, y, w, h) = slot_rect(s);
            let y = if s == slot::ARMOR as usize { y - 10.0 } else { y };
            let rect = [x + 1.0, y, w - 4.0, h - 4.0];
            out.push(ItemView { key: s as u32, item: *item, rect, picked: false, hovered: free && hovered(rect) });
        }
        for g in &inv.grid {
            let (w, h) = Inventory::size(&data, &g.item);
            let rect = [GRID.0 + g.x as f32 * SQUARE, GRID.1 + g.y as f32 * SQUARE, w as f32 * SQUARE, h as f32 * SQUARE];
            let key = 100 + g.y as u32 * COLUMNS as u32 + g.x as u32;
            out.push(ItemView { key, item: g.item, rect, picked: false, hovered: free && hovered(rect) });
        }
        if let (Some(p), Some(m)) = (inv.picked, m) {
            let (w, h) = Inventory::size(&data, &p.item);
            let (sw, sh) = (w as f32 * SQUARE, h as f32 * SQUARE);
            // `m_Pos = Mouse - m_Size / 2` (integers).
            let rect = [m.x - (sw / 2.0).floor(), m.y - (sh / 2.0).floor(), sw, sh];
            out.push(ItemView { key: 999, item: p.item, rect, picked: true, hovered: hovered(rect) });
        }
        // From the window at `POS` to the screen.
        for v in &mut out {
            v.rect[0] += inv.dx;
        }
        // The NPC shop's and the vault's items, already on the screen (shop.rs, vault.rs).
        out.extend(shop.views.iter().copied());
        out.extend(vault.views.iter().copied());
    }
    if views.views != out {
        views.views = out;
    }
    if views.render_level != opts.render_level {
        views.render_level = opts.render_level;
    }
}

// --- Drawing -----------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Tex {
    Back,
    Top,
    Left,
    Right,
    Bottom,
    SlotBoots,
    SlotHelm,
    SlotFairy,
    SlotWing,
    SlotWeaponR,
    SlotWeaponL,
    SlotArmor,
    SlotGloves,
    SlotPants,
    SlotRing,
    SlotNecklace,
    Money,
    Exit,
    Repair,
    Shop,
    Square,
    TableTopLeft,
    TableTopRight,
    TableBottomLeft,
    TableBottomRight,
    TableTop,
    TableBottom,
    TableLeft,
    TableRight,
    /// IMAGE_NPCSHOP_REPAIR_MONEY (NewUINPCShop.cpp:292).
    Money2,
    /// `CNewUIStorageInventory::LoadImages` (NewUIStorageInventory.cpp:230-242): its top
    /// (`IMAGE_STORAGE_TOP`, loaded from `newui_item_back01`), the zen box and the buttons.
    StorageTop,
    StorageMoney,
    BtnInsertZen,
    BtnTakeZen,
    BtnUnlock,
    BtnLock,
    /// `CNewUIMessageBoxMng::LoadImages` (NewUIMessageBox.cpp:447-462) for the vault's message boxes.
    MsgTop,
    MsgMiddle,
    MsgBottom,
    MsgOk,
    MsgCancel,
}

impl Tex {
    const ALL: [Tex; 41] = [
        Tex::Back,
        Tex::Top,
        Tex::Left,
        Tex::Right,
        Tex::Bottom,
        Tex::SlotBoots,
        Tex::SlotHelm,
        Tex::SlotFairy,
        Tex::SlotWing,
        Tex::SlotWeaponR,
        Tex::SlotWeaponL,
        Tex::SlotArmor,
        Tex::SlotGloves,
        Tex::SlotPants,
        Tex::SlotRing,
        Tex::SlotNecklace,
        Tex::Money,
        Tex::Exit,
        Tex::Repair,
        Tex::Shop,
        Tex::Square,
        Tex::TableTopLeft,
        Tex::TableTopRight,
        Tex::TableBottomLeft,
        Tex::TableBottomRight,
        Tex::TableTop,
        Tex::TableBottom,
        Tex::TableLeft,
        Tex::TableRight,
        Tex::Money2,
        Tex::StorageTop,
        Tex::StorageMoney,
        Tex::BtnInsertZen,
        Tex::BtnTakeZen,
        Tex::BtnUnlock,
        Tex::BtnLock,
        Tex::MsgTop,
        Tex::MsgMiddle,
        Tex::MsgBottom,
        Tex::MsgOk,
        Tex::MsgCancel,
    ];

    /// `CNewUIMyInventory::LoadImages` (NewUIMyInventory.cpp:1153-1177) and
    /// `CNewUIInventoryCtrl::LoadImages` (NewUIInventoryCtrl.cpp:192-209). The top background drawn
    /// is `newui_item_back04` (`IMAGE_INVENTORY_BACK_TOP2`).
    fn file(self) -> &'static str {
        match self {
            Tex::Back => "newui_msgbox_back.jpg",
            Tex::Top => "newui_item_back04.tga",
            Tex::Left => "newui_item_back02-L.tga",
            Tex::Right => "newui_item_back02-R.tga",
            Tex::Bottom => "newui_item_back03.tga",
            Tex::SlotBoots => "newui_item_boots.tga",
            Tex::SlotHelm => "newui_item_cap.tga",
            Tex::SlotFairy => "newui_item_fairy.tga",
            Tex::SlotWing => "newui_item_wing.tga",
            // `IMAGE_INVENTORY_ITEM_RIGHT` is "weapon(L)" and `_LEFT` is "weapon(R)".
            Tex::SlotWeaponR => "newui_item_weapon(L).tga",
            Tex::SlotWeaponL => "newui_item_weapon(R).tga",
            Tex::SlotArmor => "newui_item_upper.tga",
            Tex::SlotGloves => "newui_item_gloves.tga",
            Tex::SlotPants => "newui_item_lower.tga",
            Tex::SlotRing => "newui_item_ring.tga",
            Tex::SlotNecklace => "newui_item_necklace.tga",
            Tex::Money => "newui_item_money.tga",
            Tex::Exit => "newui_exit_00.tga",
            Tex::Repair => "newui_repair_00.tga",
            Tex::Shop => "newui_Bt_openshop.tga",
            Tex::Square => "newui_item_box.tga",
            Tex::TableTopLeft => "newui_item_table01(L).tga",
            Tex::TableTopRight => "newui_item_table01(R).tga",
            Tex::TableBottomLeft => "newui_item_table02(L).tga",
            Tex::TableBottomRight => "newui_item_table02(R).tga",
            Tex::TableTop => "newui_item_table03(Up).tga",
            Tex::TableBottom => "newui_item_table03(Dw).tga",
            Tex::TableLeft => "newui_item_table03(L).tga",
            Tex::TableRight => "newui_item_table03(R).tga",
            Tex::Money2 => "newui_item_money2.tga",
            Tex::StorageTop => "newui_item_back01.tga",
            Tex::StorageMoney => "newui_item_money3.tga",
            Tex::BtnInsertZen => "newui_Bt_money01.tga",
            Tex::BtnTakeZen => "newui_Bt_money02.tga",
            Tex::BtnUnlock => "newui_Bt_lock02.tga",
            Tex::BtnLock => "newui_Bt_lock.tga",
            Tex::MsgTop => "newui_msgbox_top.tga",
            Tex::MsgMiddle => "newui_msgbox_middle.tga",
            Tex::MsgBottom => "newui_msgbox_bottom.tga",
            Tex::MsgOk => "newui_button_ok.tga",
            Tex::MsgCancel => "newui_button_cancel.tga",
        }
    }
}

#[derive(Resource)]
struct InventoryTextures(std::collections::HashMap<Tex, Handle<Image>>);

/// A piece on the 640×480 screen.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Piece {
    /// `RenderImage`: the `src` part of the texture at (x, y, width, height) (stretched if sizes differ).
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, src: [f32; 4] },
    /// `RenderColor` with `glColor4f`.
    Color { x: f32, y: f32, w: f32, h: f32, rgba: [f32; 4] },
    /// Text with its top-left corner at (x, y); with a width, centered in it.
    Text { x: f32, y: f32, w: Option<f32>, text: String, color: [f32; 3], bold: bool, bg: Option<[f32; 4]> },
    /// Text ending at `x` (`RT3_WRITE_RIGHT_TO_LEFT`), its top at `y`.
    TextRight { x: f32, y: f32, text: String, color: [f32; 3], bold: bool },
}

impl Piece {
    /// The same piece `dx` further right.
    pub(crate) fn shifted(&self, dx: f32) -> Piece {
        let mut p = self.clone();
        match &mut p {
            Piece::Image { x, .. } | Piece::Color { x, .. } | Piece::Text { x, .. } | Piece::TextRight { x, .. } => *x += dx,
        }
        p
    }
}

/// `RenderImage(tex, x, y, w, h, 0, sv)`: unstretched from (0, sv).
pub(crate) fn img(tex: Tex, x: f32, y: f32, w: f32, h: f32, sv: f32) -> Piece {
    Piece::Image { tex, x, y, w, h, src: [0.0, sv, w, h] }
}

pub(crate) fn color(x: f32, y: f32, w: f32, h: f32, rgba: [f32; 4]) -> Piece {
    Piece::Color { x, y, w, h, rgba }
}

pub(crate) fn text(x: f32, y: f32, w: Option<f32>, s: &str, color: [f32; 3], bold: bool) -> Piece {
    Piece::Text { x, y, w, text: s.to_string(), color, bold, bg: None }
}

/// `ConvertGold`: thousands separated by commas.
pub fn convert_gold(zen: u32) -> String {
    price::convert_gold(zen as i64)
}

/// `getGoldColor` (ZzzInventory.cpp:1041-1057; ABGR): red from 10 000 000, orange from 1 000 000,
/// green from 100 000 and light yellow otherwise.
pub(crate) fn gold_color(zen: u32) -> [f32; 3] {
    let c = |r: u8, g: u8, b: u8| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
    if zen >= 10_000_000 {
        c(255, 0, 0)
    } else if zen >= 1_000_000 {
        c(255, 150, 0)
    } else if zen >= 100_000 {
        c(0, 201, 24)
    } else {
        c(255, 220, 150)
    }
}

/// Tooltip line height (`TextSize.cy` of `g_hFont` on the 480 screen).
const TIP_LINE: f32 = FONT;

/// The height `RenderTipTextList` gives the box: `(lines · cy + gaps · cy / 2) · 1.1`.
fn tip_height(lines: &[TipLine]) -> f32 {
    let (texts, gaps) = lines.iter().fold((0, 0), |(t, g), l| if l.is_gap() { (t, g + 1) } else { (t + 1, g) });
    (TIP_LINE * texts as f32 + TIP_LINE / 2.0 * gaps as f32) * 1.1
}

/// `RenderTipTextList(sx, sy, ...)` (ZzzInventory.cpp:285-453): the box centered on `sx`, from `sy` down.
fn push_tip_list(out: &mut Vec<Piece>, lines: &[TipLine], sx: f32, sy: f32, metrics: &crate::chat::TextMetrics) {
    if lines.is_empty() {
        return;
    }
    let height = tip_height(lines);
    // Bold text is slightly wider than the regular font it is measured with.
    let width = lines
        .iter()
        .filter(|l| !l.is_gap())
        .map(|l| metrics.width(&l.text) * if l.bold { 1.06 } else { 1.0 })
        .fold(0.0f32, f32::max)
        .ceil()
        + 4.0;
    let mut x = (sx - width / 2.0).floor().max(0.0);
    if x + width > 640.0 {
        x = 640.0 - width - 1.0;
    }
    let black = [0.0, 0.0, 0.0, 1.0];
    out.push(color(x - 1.0, sy - 1.0, width + 1.0, 1.0, black));
    out.push(color(x - 1.0, sy - 1.0, 1.0, height + 1.0, black));
    out.push(color(x + width, sy - 1.0, 1.0, height + 1.0, black));
    out.push(color(x - 1.0, sy + height, width + 2.0, 1.0, black));
    out.push(color(x, sy, width, height, [0.0, 0.0, 0.0, 0.8]));
    let mut y = sy;
    for l in lines {
        if l.is_gap() {
            y += TIP_LINE / 2.0 * 1.1;
            continue;
        }
        let bg = l.color.background().map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]);
        out.push(Piece::Text { x: x + 1.0, y, w: Some(width - 2.0), text: l.text.clone(), color: l.color.rgb(), bold: l.bold, bg });
        y += TIP_LINE * 1.1;
    }
}

/// [`push_tip_list`] with the `RenderItemInfo` placement (`sy += INVENTORY_SCALE` and not past 420,
/// ZzzInventory.cpp:5596-5629).
pub(crate) fn push_tooltip(out: &mut Vec<Piece>, lines: &[TipLine], sx: f32, sy: f32, metrics: &crate::chat::TextMetrics) {
    let height = tip_height(lines);
    let mut sy = sy + 20.0;
    if sy + height > 420.0 {
        sy = 420.0 - height;
    }
    push_tip_list(out, lines, sx, sy, metrics);
}

/// [`push_tip_list`] with the `RenderRepairInfo` placement (ZzzInventory.cpp:5937-5947): above the
/// point by its height without the 1.1 (`((TextNum - SkipNum) · cy + SkipNum · cy / 2)`), or below
/// the item (`p->Height · INVENTORY_SCALE`) if it does not fit.
fn push_repair_tooltip(out: &mut Vec<Piece>, lines: &[TipLine], sx: f32, sy: f32, item_height: u8, metrics: &crate::chat::TextMetrics) {
    let height = tip_height(lines) / 1.1;
    let sy = if sy - height >= 0.0 { sy - height } else { sy + item_height as f32 * SQUARE };
    push_tip_list(out, lines, sx, sy, metrics);
}

/// `CNewUIButton::Render` of the help text with `istoppos` (`ChangeToolTipText(text, true)`,
/// NewUIButton.cpp:439-460): above the button, centered, on a black background of alpha 180.
pub(crate) fn push_button_tip(out: &mut Vec<Piece>, s: &str, bx: f32, by: f32, metrics: &crate::chat::TextMetrics) {
    let tw = metrics.width(s).ceil();
    let mut x = (bx + BTN_W / 2.0 - tw / 2.0).floor();
    let y = by - (FONT + 2.0);
    if x + tw + 6.0 > 640.0 {
        x -= x + tw + 6.0 - 640.0;
    }
    out.push(Piece::Text { x, y, w: Some(tw + 6.0), text: s.to_string(), color: [1.0; 3], bold: false, bg: Some([0.0, 0.0, 0.0, 180.0 / 255.0]) });
}

/// `CNewUIInventoryCtrl::Render` (NewUIInventoryCtrl.cpp:862-939) of a `cols`×`rows` grid at
/// `origin`: the occupied cells, the squares and the table border.
pub(crate) fn push_grid_frame(out: &mut Vec<Piece>, origin: (f32, f32), cols: usize, rows: usize, occupied: &dyn Fn(usize, usize) -> bool) {
    let (gx, gy) = origin;
    for y in 0..rows {
        for x in 0..cols {
            let (px, py) = (gx + x as f32 * SQUARE, gy + y as f32 * SQUARE);
            if occupied(x, y) {
                // `ITEM_COLOR_NORMAL`.
                out.push(color(px, py, SQUARE, SQUARE, [0.3, 0.5, 0.5, 0.6]));
            }
            out.push(img(Tex::Square, px, py, 21.0, 21.0, 0.0));
        }
    }
    let (sw, sh) = (cols as f32 * SQUARE, rows as f32 * SQUARE);
    out.push(img(Tex::TableTopLeft, gx - EDGE_LEFT, gy - EDGE_TOP, 14.0, 14.0, 0.0));
    out.push(img(Tex::TableTopRight, gx + sw - EDGE_RIGHT, gy - EDGE_TOP, 14.0, 14.0, 0.0));
    out.push(img(Tex::TableBottomLeft, gx - EDGE_LEFT, gy + sh - EDGE_BOTTOM, 14.0, 14.0, 0.0));
    out.push(img(Tex::TableBottomRight, gx + sw - EDGE_RIGHT, gy + sh - EDGE_BOTTOM, 14.0, 14.0, 0.0));
    // One-pixel borders side by side: the 1-pixel strip stretched.
    let (x0, x1) = (gx - EDGE_LEFT + 14.0, gx + sw - EDGE_RIGHT);
    let (y0, y1) = (gy - EDGE_TOP + 14.0, gy + sh - EDGE_BOTTOM);
    let strip = |tex, x, y, w, h, src: [f32; 4]| Piece::Image { tex, x, y, w, h, src };
    out.push(strip(Tex::TableTop, x0, gy - EDGE_TOP, x1 - x0, 14.0, [0.0, 0.0, 1.0, 14.0]));
    out.push(strip(Tex::TableBottom, x0, gy + sh - EDGE_BOTTOM, x1 - x0, 14.0, [0.0, 0.0, 1.0, 14.0]));
    out.push(strip(Tex::TableLeft, gx - EDGE_LEFT, y0, 14.0, y1 - y0, [0.0, 0.0, 14.0, 1.0]));
    out.push(strip(Tex::TableRight, gx + sw - EDGE_RIGHT, y0, 14.0, y1 - y0, [0.0, 0.0, 14.0, 1.0]));
}

/// The held item over a grid (NewUIInventoryCtrl.cpp:940-1113): where it would land, in `normal`
/// (`m_afColorStateNormal`, (0.1, 0.4, 0.8) unless a window changes it) or red where it does not fit.
/// `size` is the held item's, `m` the mouse.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_drop_overlay(
    out: &mut Vec<Piece>,
    origin: (f32, f32),
    cols: usize,
    rows: usize,
    size: (u8, u8),
    m: Vec2,
    occupied: &dyn Fn(usize, usize) -> bool,
    normal: [f32; 3],
) {
    let (gx, gy) = origin;
    let (sw, sh) = (cols as f32 * SQUARE, rows as f32 * SQUARE);
    let (w, h) = size;
    let (pw, ph) = (w as f32 * SQUARE, h as f32 * SQUARE);
    let (px, py) = (m.x - (pw / 2.0).floor(), m.y - (ph / 2.0).floor());
    let intersects = px < gx + sw && px + pw > gx && py < gy + sh && py + ph > gy;
    if !intersects {
        return;
    }
    let ix = m.x as i32 - (w as i32 - 1) * SQUARE as i32 / 2;
    let iy = m.y as i32 - (h as i32 - 1) * SQUARE as i32 / 2;
    // Outside the grid, the cell index (negative or past the end) counted from its corner.
    let cell = |v: i32, origin: f32| {
        let d = v - origin as i32;
        if d < 0 { d / SQUARE as i32 - 1 } else { d / SQUARE as i32 }
    };
    let (mut cx, mut cy) = (cell(ix, gx), cell(iy, gy));
    let (mut nw, mut nh) = (w as i32, h as i32);
    let mut warning = false;
    let (cols, rows) = (cols as i32, rows as i32);
    if cx < 0 && cx >= -nw {
        nw += cx;
        cx = 0;
        warning = true;
    }
    if cx + nw > cols && cx < cols {
        nw = cols - cx;
        warning = true;
    }
    if cy < 0 && cy >= -nh {
        nh += cy;
        cy = 0;
        warning = true;
    }
    if cy + nh > rows && cy < rows {
        nh = rows - cy;
        warning = true;
    }
    if warning {
        out.push(color(gx + cx as f32 * SQUARE, gy + cy as f32 * SQUARE, nw as f32 * SQUARE, nh as f32 * SQUARE, [1.0, 0.2, 0.2, 0.4]));
    } else if (0..cols).contains(&cx) && (0..rows).contains(&cy) {
        for dy in 0..nh {
            for dx in 0..nw {
                let (sx, sy) = (cx + dx, cy + dy);
                let busy = occupied(sx as usize, sy as usize);
                // `m_afColorStateWarning` (1, 0.2, 0.2) or `m_afColorStateNormal`, at 0.4.
                let c = if busy { [1.0, 0.2, 0.2, 0.4] } else { [normal[0], normal[1], normal[2], 0.4] };
                out.push(color(gx + sx as f32 * SQUARE, gy + sy as f32 * SQUARE, SQUARE, SQUARE, c));
            }
        }
    }
}

/// `m_afColorStateNormal` (NewUIInventoryCtrl.cpp:187-190).
pub(crate) const DROP_NORMAL: [f32; 3] = [0.1, 0.4, 0.8];

impl Inventory {
    /// `CNewUIMyInventory::Render` (everything behind the items). `own_buttons`: the repair and
    /// personal store buttons are shown (see [`own_buttons`]).
    #[allow(clippy::too_many_arguments)]
    fn back_pieces(
        &self,
        data: &ItemData,
        hero: &HeroView,
        zen: u32,
        m: Option<Vec2>,
        own_buttons: bool,
        metrics: &crate::chat::TextMetrics,
    ) -> Vec<Piece> {
        let mut out = Vec::new();
        let (x, y) = POS;
        // `RenderFrame`.
        out.push(img(Tex::Back, x, y, W, H, 0.0));
        out.push(img(Tex::Top, x, y, W, 64.0, 0.0));
        out.push(img(Tex::Left, x, y + 64.0, 21.0, 320.0, 0.0));
        out.push(img(Tex::Right, x + W - 21.0, y + 64.0, 21.0, 320.0, 0.0));
        out.push(img(Tex::Bottom, x, y + H - 45.0, W, 45.0, 0.0));
        // `RenderInventoryDetails`: title and zen in bold.
        out.push(text(x, y + 12.0, Some(W), TITLE, [1.0; 3], true));
        out.push(img(Tex::Money, x + 11.0, y + 364.0, 170.0, 26.0, 0.0));
        out.push(text(x + 50.0, y + 371.0, None, &convert_gold(zen), gold_color(zen), true));
        // `RenderSetOption` and `RenderSocketOption`: gray without sets or sockets.
        let gray = [100.0 / 255.0; 3];
        out.push(text(x + W * 0.2, y + 25.0, Some(W * 0.3), SET_OPTION, gray, true));
        out.push(text(x + W * 0.5, y + 25.0, Some(W * 0.3), SOCKET_OPTION, gray, true));
        // `RenderButtons`: repair from level 50 and the store from level 6 (`OpenningProcess`).
        let over = |bx: f32, by: f32| m.is_some_and(|m| inside(m, bx, by, BTN_W, BTN_H));
        let mut tips = Vec::new();
        if own_buttons && hero.level >= 50 {
            let frame = if self.repair_down && over(BTN_REPAIR.0, BTN_REPAIR.1) { BTN_H } else { 0.0 };
            out.push(img(Tex::Repair, BTN_REPAIR.0, BTN_REPAIR.1, BTN_W, BTN_H, frame));
            if over(BTN_REPAIR.0, BTN_REPAIR.1) {
                tips.push((TIP_REPAIR, BTN_REPAIR));
            }
        }
        if own_buttons && hero.level >= 6 {
            out.push(img(Tex::Shop, BTN_SHOP.0, BTN_SHOP.1, BTN_W, BTN_H, 0.0));
            if over(BTN_SHOP.0, BTN_SHOP.1) {
                tips.push((TIP_SHOP, BTN_SHOP));
            }
        }
        let exit_frame = if self.exit_down && over(BTN_EXIT.0, BTN_EXIT.1) { BTN_H } else { 0.0 };
        out.push(img(Tex::Exit, BTN_EXIT.0, BTN_EXIT.1, BTN_W, BTN_H, exit_frame));
        if over(BTN_EXIT.0, BTN_EXIT.1) {
            tips.push((TIP_EXIT, BTN_EXIT));
        }
        for (s, (bx, by)) in tips {
            push_button_tip(&mut out, s, bx, by, metrics);
        }
        let occupied = |x: usize, y: usize| self.owner(data, x, y).is_some();
        push_grid_frame(&mut out, GRID, COLUMNS, ROWS, &occupied);
        if let (Some(p), Some(m)) = (self.picked, m) {
            push_drop_overlay(&mut out, GRID, COLUMNS, ROWS, Self::size(data, &p.item), m, &occupied, DROP_NORMAL);
        }
        self.push_equipment(&mut out, data, hero);
        out
    }

    /// `RenderEquippedItem` (NewUIMyInventory.cpp:1214-1299).
    fn push_equipment(&self, out: &mut Vec<Piece>, data: &ItemData, hero: &HeroView) {
        for i in 0..slot::COUNT {
            // The Magic Gladiator wears no helm and the Rage Fighter no gloves.
            if (i == slot::HELM as usize && hero.class == 3) || (i == slot::GLOVES as usize && hero.class == 6) {
                continue;
            }
            let (x, y, w, h) = slot_rect(i);
            out.push(img(SLOTS[i].4, x, y, w, h, 0.0));
            let Some(item) = self.equipment[i] else { continue };
            let Some(attr) = data.attr(&item) else { continue };
            let max = mu_data::items::max_durability(attr, &item) as f32;
            let d = item.durability as f32;
            let c = if item.durability == 0 {
                Some([1.0, 0.0, 0.0, 0.25])
            } else if d <= max * 0.2 {
                Some([1.0, 0.15, 0.0, 0.25])
            } else if d <= max * 0.3 {
                Some([1.0, 0.5, 0.0, 0.25])
            } else if d <= max * 0.5 {
                Some([1.0, 1.0, 0.0, 0.25])
            } else if !is_equipable(self, data, hero, i, &item) {
                Some([1.0, 0.0, 0.0, 0.25])
            } else {
                None
            };
            if let Some(c) = c {
                out.push(color(x + 1.0, y, w - 4.0, h - 4.0, c));
            }
        }
        // Holding an item over an occupied or unsuitable slot: red.
        if let (Some(p), Some(s)) = (self.picked, self.pointed_slot) {
            if self.equipment[s].is_some() || !is_equipable(self, data, hero, s, &p.item) {
                let (x, y, w, h) = slot_rect(s);
                out.push(color(x + 1.0, y, w - 4.0, h - 4.0, [0.9, 0.1, 0.1, 0.4]));
            }
        }
    }

    /// Tooltips (`RenderItemToolTip` of the grid and the equipment): the item info, with the
    /// selling price while the NPC shop is open (`shop_open`), or in repair mode the repair info
    /// (`TOOLTIP_TYPE_REPAIR`, NewUIInventoryCtrl.cpp:1419-1422, and NewUIMyInventory.cpp:2246-2253)
    /// priced for `place`.
    fn front_pieces(&self, data: &ItemData, hero: &HeroView, shop_open: bool, place: price::RepairPlace, metrics: &crate::chat::TextMetrics) -> Vec<Piece> {
        let mut out = Vec::new();
        let mana = |s: u16| data.skill_mana.as_ref().and_then(|m| m.get(s as usize)).map(|&m| m as u32);
        let tip = |out: &mut Vec<Piece>, attr: &mu_data::items::ItemAttribute, item: &Item, sx: f32, sy: f32| {
            if self.repair_mode {
                if let Some((lines, _)) = price::repair_info(attr, item, hero, place) {
                    push_repair_tooltip(out, &lines, sx, sy, attr.height, metrics);
                }
                return;
            }
            let mut lines = item_info(attr, item, hero, &mana);
            if shop_open && let Some(line) = price::price_line(attr, item, false, 0) {
                price::add_price_line(&mut lines, line);
            }
            push_tooltip(out, &lines, sx, sy, metrics);
        };
        // `CNewUIInventoryCtrl::RenderItemToolTip`: centered over the item, from its top row
        // (half a cell lower for one-high items).
        if let (Some(i), None) = (self.tooltip, self.picked)
            && let Some(g) = self.grid.get(i)
            && let Some(attr) = data.attr(&g.item)
        {
            let sx = GRID.0 + g.x as f32 * SQUARE + attr.width as f32 * SQUARE / 2.0;
            let mut sy = GRID.1 + g.y as f32 * SQUARE;
            if attr.height == 1 {
                sy += SQUARE / 2.0;
            }
            tip(&mut out, attr, &g.item, sx.floor(), sy);
        }
        // `CNewUIMyInventory::RenderItemToolTip`: the slot's, from its center.
        if let Some(s) = self.pointed_slot
            && let Some(item) = self.equipment[s]
            && let Some(attr) = data.attr(&item)
        {
            let (x, y, w, h) = slot_rect(s);
            tip(&mut out, attr, &item, (x + w / 2.0).floor(), (y + h / 2.0).floor());
        }
        out
    }
}

#[derive(Component)]
struct InventoryRoot;

/// The window's three layers: background, 3D items and tooltips.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Back,
    Items,
    Front,
}

fn full_screen() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        top: Val::Px(0.0),
        width: Val::Px(640.0),
        height: Val::Px(480.0),
        ..default()
    }
}

fn spawn_root(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let textures = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|(h, _)| (t, h)))
        .collect();
    commands.insert_resource(InventoryTextures(textures));
    // The 640×480 screen centered and bottom-aligned, like the bar (options.rs).
    commands
        .spawn((
            Name::new("Inventario"),
            InventoryRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-320.0)),
                width: Val::Px(640.0),
                height: Val::Px(480.0),
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(1),
        ))
        .with_children(|p| {
            p.spawn((full_screen(), Layer::Back));
            // Transparent until the item camera's image exists.
            p.spawn((full_screen(), Layer::Items, ImageNode { color: Color::NONE, ..default() }));
            p.spawn((full_screen(), Layer::Front));
        });
}

fn absolute(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(x),
        top: Val::Px(y),
        width: Val::Px(w),
        height: Val::Px(h),
        ..default()
    }
}

fn spawn_pieces(p: &mut ChildSpawnerCommands, pieces: &[Piece], textures: &InventoryTextures) {
    let rgb = |c: [f32; 3]| Color::srgb(c[0], c[1], c[2]);
    for piece in pieces {
        match piece {
            Piece::Image { tex, x, y, w, h, src } => {
                let Some(handle) = textures.0.get(tex) else { continue };
                let mut image = ImageNode::new(handle.clone());
                image.rect = Some(Rect::new(src[0], src[1], src[0] + src[2], src[1] + src[3]));
                p.spawn((absolute(*x, *y, *w, *h), image));
            }
            Piece::Color { x, y, w, h, rgba } => {
                p.spawn((absolute(*x, *y, *w, *h), BackgroundColor(Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]))));
            }
            Piece::Text { x, y, w, text, color, bold, bg } => {
                let mut font = TextFont { font_size: FontSize::Px(FONT), font_smoothing: FontSmoothing::AntiAliased, ..default() };
                if *bold {
                    font.weight = FontWeight::BOLD;
                }
                let label = (Text::new(text.clone()), font, LineHeight::Px(FONT), TextColor(rgb(*color)), TextLayout::no_wrap());
                let node = match w {
                    Some(w) => {
                        let mut n = absolute(*x, *y, *w, FONT);
                        n.justify_content = JustifyContent::Center;
                        n
                    }
                    None => Node { position_type: PositionType::Absolute, left: Val::Px(*x), top: Val::Px(*y), ..default() },
                };
                let mut e = p.spawn(node);
                if let Some(c) = bg {
                    e.insert(BackgroundColor(Color::srgba(c[0], c[1], c[2], c[3])));
                }
                e.with_children(|c| {
                    c.spawn(label);
                });
            }
            Piece::TextRight { x, y, text, color, bold } => {
                let mut font = TextFont { font_size: FontSize::Px(FONT), font_smoothing: FontSmoothing::AntiAliased, ..default() };
                if *bold {
                    font.weight = FontWeight::BOLD;
                }
                // The layers are 640 wide: `right` counts from their right edge.
                let node = Node { position_type: PositionType::Absolute, right: Val::Px(640.0 - *x), top: Val::Px(*y), ..default() };
                p.spawn(node).with_children(|c| {
                    c.spawn((Text::new(text.clone()), font, LineHeight::Px(FONT), TextColor(rgb(*color)), TextLayout::no_wrap()));
                });
            }
        }
    }
}

/// Rebuilds the layers when what is drawn changes.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    windows: Query<&Window>,
    inv: Res<Inventory>,
    data: Res<ItemData>,
    views: Res<ItemViews>,
    textures: Option<Res<InventoryTextures>>,
    metrics: Res<crate::chat::TextMetrics>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    (shop, shop_draw, dialog, vault_draw): (Res<crate::shop::NpcShop>, Res<crate::shop::ShopDraw>, Res<crate::npc::NpcDialog>, Res<crate::vault::VaultDraw>),
    mut roots: Query<&mut Visibility, With<InventoryRoot>>,
    mut layers: Query<(Entity, &Layer, Option<&mut ImageNode>, &mut Node)>,
    mut last: Local<(Vec<Piece>, Vec<Piece>, bool)>,
) {
    let Some(textures) = textures else { return };
    let Ok(mut vis) = roots.single_mut() else { return };
    let want = if inv.open { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if !inv.open {
        return;
    }
    let m = windows.single().ok().and_then(crate::mainframe::cursor_virtual).map(|c| inv.local(c.floor()));
    let hero = hero_view(hero.as_deref(), info.as_deref());
    let zen = info.as_ref().map_or(inv.zen, |i| i.0.money);
    let mut back = inv.back_pieces(&data, &hero, zen, m, own_buttons(&dialog), &metrics);
    let mut front = inv.front_pieces(&data, &hero, shop.open, shop.repair_place(inv.open), &metrics);
    // The NPC shop draws in these layers too (so its items go between its background and its
    // tooltips); its pieces are on the screen, the layers are shifted by `dx`.
    back.extend(shop_draw.back.iter().map(|p| p.shifted(-inv.dx)));
    front.extend(shop_draw.front.iter().map(|p| p.shifted(-inv.dx)));
    // The vault likewise (vault.rs), with its message boxes on top of everything.
    back.extend(vault_draw.back.iter().map(|p| p.shifted(-inv.dx)));
    front.extend(vault_draw.front.iter().map(|p| p.shifted(-inv.dx)));
    // Layers are created once (`spawn_root`); the first time they are drawn even if nothing changed.
    let fresh = !last.2;
    for (e, layer, image, mut node) in &mut layers {
        // Background and foreground move with the window; the items image is already shifted.
        let left = Val::Px(if *layer == Layer::Items { 0.0 } else { inv.dx });
        if node.left != left {
            node.left = left;
        }
        match layer {
            Layer::Back if fresh || last.0 != back => {
                commands.entity(e).despawn_children();
                commands.entity(e).with_children(|p| spawn_pieces(p, &back, &textures));
            }
            Layer::Front if fresh || last.1 != front => {
                commands.entity(e).despawn_children();
                commands.entity(e).with_children(|p| spawn_pieces(p, &front, &textures));
            }
            Layer::Items => {
                if let (Some(mut node), Some(h)) = (image, views.image.as_ref())
                    && node.image != *h
                {
                    node.image = h.clone();
                    node.color = Color::WHITE;
                }
            }
            _ => {}
        }
    }
    *last = (back, front, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> ItemData {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data");
        ItemData { table: mu_data::items::load(&dir).ok(), ..Default::default() }
    }

    #[test]
    fn zen_con_comas() {
        assert_eq!(convert_gold(0), "0");
        assert_eq!(convert_gold(999), "999");
        assert_eq!(convert_gold(2_500_000), "2,500,000");
        assert_eq!(convert_gold(12_345), "12,345");
    }

    #[test]
    fn casillas_y_huecos() {
        let data = data();
        if data.table.is_none() {
            return;
        }
        let mut inv = Inventory::default();
        // A 2×3 armor in the first cell, so a 1×3 sword no longer fits at (1, 0).
        assert!(inv.add(&data, Item::new(group::ARMOR, 1)));
        assert_eq!(inv.grid[0].x, 0);
        assert!(!inv.fits(&data, 1, 0, 1, 3));
        assert!(inv.fits(&data, 2, 0, 1, 3));
        // Nothing sticks out to the right or the bottom.
        assert!(!inv.fits(&data, 7, 0, 2, 2));
        assert!(!inv.fits(&data, 0, 6, 1, 3));
        assert_eq!(inv.owner(&data, 1, 2), Some(0));
        assert_eq!(inv.owner(&data, 2, 2), None);
        // Equipping: the sword in the Dark Knight's right hand, not the helm slot; the shield on the left.
        let knight = HeroView { class: 1, step: 1, level: 400, stats: None };
        let sword = Item::new(group::SWORD, 1);
        assert!(is_equipable(&inv, &data, &knight, slot::WEAPON_RIGHT as usize, &sword));
        assert!(is_equipable(&inv, &data, &knight, slot::WEAPON_LEFT as usize, &sword));
        assert!(!is_equipable(&inv, &data, &knight, slot::HELM as usize, &sword));
        // A Dark Wizard cannot wear the bronze helm; a ring fits both ring slots.
        let wizard = HeroView { class: 0, ..knight };
        assert!(!is_equipable(&inv, &data, &wizard, slot::HELM as usize, &Item::new(group::HELM, 0)));
        let ring = Item::new(group::HELPER, 8);
        assert!(is_equipable(&inv, &data, &wizard, slot::RING_LEFT as usize, &ring));
        assert!(is_equipable(&inv, &data, &wizard, slot::RING_RIGHT as usize, &ring));
        // Missing strength (with server data).
        let weak = HeroView { stats: Some([10, 10, 10, 10, 0]), ..knight };
        assert!(!is_equipable(&inv, &data, &weak, slot::WEAPON_RIGHT as usize, &sword));
    }

    #[test]
    fn paquete_del_inventario() {
        let mut inv = Inventory::default();
        let sword = Item { level: 7, ..Item::new(group::SWORD, 1) };
        let jewel = Item::new(group::POTION, 13);
        inv.set_items(&[(0, sword.to_bytes()), (12 + 9, jewel.to_bytes())]);
        assert_eq!(inv.equipment[0], Some(sword));
        assert_eq!(inv.grid, vec![GridItem { x: 1, y: 1, item: jewel }]);
    }

    #[test]
    fn coger_y_devolver() {
        let mut inv = Inventory { open: true, ..Default::default() };
        let it = Item::new(group::SWORD, 0);
        inv.grid.push(GridItem { x: 3, y: 2, item: it });
        let g = inv.grid.remove(0);
        inv.picked = Some(Picked { item: g.item, from: Source::Grid { x: g.x, y: g.y } });
        inv.close();
        assert!(inv.picked.is_none());
        assert_eq!(inv.grid, vec![GridItem { x: 3, y: 2, item: it }]);
        assert!(!inv.open);
    }

    fn kris(level: u8) -> Item {
        Item { level, durability: 30, ..Item::new(group::SWORD, 1) }
    }

    #[test]
    fn slots_and_cells() {
        assert_eq!(grid_cell(12), Some((0, 0)));
        assert_eq!(grid_cell(12 + 8 * 2 + 3), Some((3, 2)));
        assert_eq!(grid_cell(75), Some((7, 7)));
        assert_eq!(grid_cell(76), None);
        assert_eq!(grid_cell(11), None);
        assert_eq!(grid_slot(3, 2), 31);
        assert_eq!(source_slot(Source::Grid { x: 3, y: 2 }), 31);
        assert_eq!(source_slot(Source::Equipment(0)), 0);
    }

    #[test]
    fn inventory_from_the_server() {
        let mut inv = Inventory::default();
        inv.grid.push(GridItem { x: 0, y: 0, item: kris(0) }); // leftover from a test inventory
        let rev = inv.equipment_rev;
        inv.apply(&ServerMessage::InventoryList(vec![(0, kris(9).to_bytes()), (31, kris(3).to_bytes()), (204, kris(1).to_bytes())]));
        assert_eq!(inv.equipment[0], Some(kris(9)));
        // Slot 204 is the personal store: not in the grid.
        assert_eq!(inv.grid, vec![GridItem { x: 3, y: 2, item: kris(3) }]);
        assert!(inv.equipment_rev > rev, "the hero is re-dressed");
    }

    /// A move waits for the server: the item stays on the mouse and cannot go back meanwhile.
    #[test]
    fn move_waits_for_the_server() {
        let mut inv = Inventory { open: true, ..Default::default() };
        inv.picked = Some(Picked { item: kris(5), from: Source::Grid { x: 0, y: 0 } });
        inv.pending = true; // `request_move` sent C3 24
        inv.close();
        assert!(inv.picked.is_some(), "BackupPickedItem does nothing while EquipmentItem");
        assert!(inv.grid.is_empty());

        // Accepted: the item goes where the server says (equipment slot 0).
        assert_eq!(inv.apply(&ServerMessage::ItemMoved { storage: storage::INVENTORY, slot: 0, item: kris(5).to_bytes() }), Some("SOUND_GET_ITEM01"));
        assert!(inv.picked.is_none() && !inv.pending);
        assert_eq!(inv.equipment[0], Some(kris(5)));
        assert!(inv.grid.is_empty());
    }

    #[test]
    fn refused_move_goes_back() {
        let mut inv = Inventory::default();
        inv.picked = Some(Picked { item: kris(5), from: Source::Grid { x: 4, y: 1 } });
        inv.pending = true;
        inv.apply(&ServerMessage::ItemMoveFailed);
        assert!(inv.picked.is_none() && !inv.pending);
        assert_eq!(inv.grid, vec![GridItem { x: 4, y: 1, item: kris(5) }]);
    }

    #[test]
    fn removed_and_durability() {
        let mut inv = Inventory::default();
        inv.apply(&ServerMessage::InventoryList(vec![(2, kris(0).to_bytes()), (13, kris(1).to_bytes())]));
        inv.apply(&ServerMessage::ItemDurability { slot: 13, durability: 7, by_consumption: true });
        assert_eq!(inv.grid[0].item.durability, 7);
        inv.apply(&ServerMessage::ItemRemoved { slot: 13 });
        assert!(inv.grid.is_empty());
        inv.apply(&ServerMessage::ItemRemoved { slot: 2 });
        assert_eq!(inv.equipment[2], None);
    }

    /// A drop on the ground (ground.rs) waits for `ReceiveDropItem`: accepted, the held item is gone;
    /// refused, it goes back; meanwhile it cannot be moved.
    #[test]
    fn drop_waits_for_the_server() {
        let mut inv = Inventory { open: true, ..Default::default() };
        let mut net = crate::net::Net::offline();
        inv.picked = Some(Picked { item: kris(5), from: Source::Grid { x: 3, y: 2 } });
        inv.drop_pending = Some(31);
        assert!(!inv.send_move(&mut net, storage::INVENTORY, 31, kris(5), storage::INVENTORY, 12), "no move while the drop waits");
        // Refused: back to its cell.
        inv.apply(&ServerMessage::ItemDropResult { success: false, slot: 31 });
        assert_eq!((inv.picked, inv.drop_pending), (None, None));
        assert_eq!(inv.grid, vec![GridItem { x: 3, y: 2, item: kris(5) }]);

        // Accepted: gone from the mouse and from the inventory.
        let g = inv.grid.remove(0);
        inv.picked = Some(Picked { item: g.item, from: Source::Grid { x: g.x, y: g.y } });
        inv.drop_pending = Some(31);
        inv.apply(&ServerMessage::ItemDropResult { success: true, slot: 31 });
        assert_eq!((inv.picked, inv.drop_pending), (None, None));
        assert!(inv.grid.is_empty());

        // The window closed while it waited (`BackupPickedItem`): the answer removes it from its cell.
        inv.grid.push(GridItem { x: 3, y: 2, item: kris(5) });
        inv.drop_pending = Some(31);
        inv.apply(&ServerMessage::ItemDropResult { success: true, slot: 31 });
        assert!(inv.grid.is_empty());

        // An equipped item: the slot is already empty since it was picked up.
        inv.picked = Some(Picked { item: kris(5), from: Source::Equipment(0) });
        inv.drop_pending = Some(0);
        inv.apply(&ServerMessage::ItemDropResult { success: true, slot: 0 });
        assert_eq!((inv.picked, inv.equipment[0]), (None, None));

        // Another item held after the dropped one went back stays on the mouse.
        inv.picked = Some(Picked { item: kris(1), from: Source::Grid { x: 0, y: 0 } });
        inv.apply(&ServerMessage::ItemDropResult { success: true, slot: 31 });
        assert!(inv.picked.is_some());
    }

    /// `ReceiveGetItem` with a slot: `InsertItem` (WSclient.cpp:5629-5633).
    #[test]
    fn picked_up_into_the_grid() {
        let mut inv = Inventory::default();
        inv.apply(&ServerMessage::ItemPickedUp { slot: 14, item: kris(2).to_bytes() });
        assert_eq!(inv.grid, vec![GridItem { x: 2, y: 0, item: kris(2) }]);
        // Equipment slots are not filled by a pick-up.
        inv.apply(&ServerMessage::ItemPickedUp { slot: 0, item: kris(2).to_bytes() });
        assert_eq!(inv.equipment[0], None);
    }
}
