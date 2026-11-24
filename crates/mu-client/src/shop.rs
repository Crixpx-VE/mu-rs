//! The NPC shop window: `SEASON3B::CNewUINPCShop` (NewUINPCShop.cpp) with its 8×15 locked
//! `CNewUIInventoryCtrl` grid.
//!
//! - **Opening and closing**: it opens when npc.rs has a shop dialog (`ReceiveTalk` → `Show(
//!   INTERFACE_NPCSHOP)`, which also shows the inventory, NewUISystem.cpp:659-665) and fills its grid
//!   from the store list (`ReceiveTradeInventory` → `InsertItem`, WSclient.cpp:1401-1404). Closing (its
//!   top X, Esc, walking...) goes through npc.rs, which sends `SendExitInventory`.
//! - **Drawing** at (260, 0) (`NPCSHOP_POS_X/Y`, NewUINPCShop.h:34-35): the inventory's frame,
//!   "Merchant" and the tax rate (`RenderTexts`), the repair buttons and the "Repair all" cost of a
//!   repair shop (`RenderButton`, `RenderRepairMoney`) and the grid; the items in 3D (item_view.rs)
//!   and their tooltips with the buying price (`TOOLTIP_TYPE_NPC_SHOP` → `RenderItemInfo(..., true)`).
//!   It draws inside the inventory's layers (inventory.rs), so its items sit between its background
//!   and its tooltips.
//! - **Buying** (`UpdateMouseEvent`, NewUINPCShop.cpp:88-164): releasing the left button over a shop
//!   item sends `SendRequestBuy` (C3 32), one at a time (`BuyCost`, wsclientinline.h:720-732); the
//!   item goes into the inventory when the server says where (`ReceiveBuy`, WSclient.cpp:6003-6030,
//!   handled by inventory.rs).
//! - **Selling** (`InventoryProcess`, NewUINPCShop.cpp:326-386): an inventory item released over the
//!   shop grid sends `SendRequestSell` (C3 33) and stays on the mouse until the answer
//!   (`ReceiveSell`, WSclient.cpp:6032-6064): sold, it disappears and the zen becomes the server's;
//!   refused, it goes back.
//! - **Repair** (repair shops, `SetRepairShop`): its repair button or `L` switch the shop to
//!   `SHOP_STATE_REPAIR` and the inventory to repair mode (`ToggleState`, NewUINPCShop.cpp:471-484);
//!   "Repair all" and `Shift+L` send `SendRequestRepair(255, 0)`.
//!
//! The server is the only authority over items and zen: nothing here changes them by itself.
//!
//! Not yet: the gamble shop's purchase box (`CGambleBuyMsgBoxLayout`, NewUINPCShop.cpp:111-119) and
//! the confirmation box before selling a valuable item (`CHighValueItemCheckMsgBoxLayout`,
//! NewUICommonMessageBox.cpp:2503-2569); there is no message box framework yet, so the gamble shop
//! does not buy and a valuable item goes back to the inventory (the box's "Cancel"). The tax rate
//! (`ReceiveTaxInfo`, C1 B2 1A, WSclient.cpp:10475-10492) is not decoded: it stays 0. The harmony
//! option check before selling (`Jewel_Of_Harmony_Option`, NewUINPCShop.cpp:348-353) needs the
//! harmony byte, which `Item` does not keep yet.

use bevy::prelude::*;
use mu_data::items::info::item_info;
use mu_data::items::price::{self, GoldType, RepairPlace};
use mu_data::items::Item;
use mu_protocol::game::{ClientMessage, ServerMessage};

use crate::chat_log::MsgType;
use crate::inventory::{self, BTN_H, BTN_W, DROP_NORMAL, GridItem, Inventory, InventorySet, Piece, SQUARE, Tex, inside};
use crate::item_view::{ItemData, ItemView};
use crate::npc::{NpcDialog, NpcDialogSystems, WindowKind};
use crate::scene::GameScene;
use crate::sound::SoundQueue;

pub struct ShopPlugin;

impl Plugin for ShopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NpcShop>()
            .init_resource::<ShopDraw>()
            .add_systems(PreUpdate, (server_messages, follow_dialog).chain().after(NpcDialogSystems).after(crate::net::NetSystems))
            .add_systems(
                PreUpdate,
                keys.after(bevy::input::InputSystems).after(crate::chat::ChatKeys).run_if(in_state(GameScene::Game)),
            )
            .add_systems(
                Update,
                (update_mouse, prepare_draw)
                    .chain()
                    .after(InventorySet::Mouse)
                    .before(InventorySet::Draw)
                    .run_if(in_state(GameScene::Game)),
            )
            .add_systems(OnExit(GameScene::Game), |mut shop: ResMut<NpcShop>, mut draw: ResMut<ShopDraw>| {
                *shop = NpcShop::default();
                *draw = ShopDraw::default();
            });
    }
}

// --- Measurements (640×480 screen) -------------------------------------------------------------

/// `NPCSHOP_POS_X`, `NPCSHOP_POS_Y` (NewUINPCShop.h:34-35), `NPCSHOP_WIDTH` × `NPCSHOP_HEIGHT` (43-44).
const POS: (f32, f32) = (260.0, 0.0);
const W: f32 = 190.0;
const H: f32 = 429.0;
/// The grid: `Create(..., x + 15, y + 50, 8, 15)` (NewUINPCShop.cpp:46).
const GRID: (f32, f32) = (POS.0 + 15.0, POS.1 + 50.0);
pub const COLUMNS: usize = 8;
pub const ROWS: usize = 15;
/// `SetButtonInfo` (NewUINPCShop.cpp:450-459): repair and repair all, 36×29.
const BTN_REPAIR: (f32, f32) = (POS.0 + 54.0, POS.1 + 390.0);
const BTN_REPAIR_ALL: (f32, f32) = (POS.0 + 98.0, POS.1 + 390.0);
/// The top X (`BtnProcess`, NewUINPCShop.cpp:390-397).
const BTN_CLOSE: (f32, f32, f32, f32) = (POS.0 + 169.0, POS.1 + 7.0, 13.0, 12.0);
/// `if (CharacterMachine->Gold + ItemValue(pItem) > 2000000000)` (NewUINPCShop.cpp:341).
const MAX_ZEN: i64 = 2_000_000_000;

/// Texts: `GlobalText[230, 1623, 233, 237, 239, 732, 733, 668, 3148]`, in English from MuMain's
/// Localization/Game.en.resx (lines 640, 4785, 649, 661, 667, 2433, 2436, 1776, 8022; the indices
/// from the call sites MuMain shares with Main 5.2: UI/NPCs/NPCShop.cpp:311-383 and
/// Network/Server/WSclient.cpp ReceiveBuy/ReceiveSell).
const TITLE: &str = "Merchant";
fn tax_text(rate: i32) -> String {
    format!("Tax rate: {rate}% (changed in real-time)")
}
const TIP_REPAIR_ALL: &str = "Repair all (A)";
const REPAIR_ALL: &str = "Repair all";
const CANNOT_BE_TRADED: &str = "Cannot be traded.";
const CANNOT_BE_SOLD: &str = "Cannot be sold.";
const ITEMS_CANNOT_BE_TRADED: &str = "These items cannot be traded.";
const TOO_MUCH_ZEN: &str = "Exceeded maximum amount of Zen you can possess.";

// --- State -------------------------------------------------------------------------------------

/// `m_dwShopState`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ShopState {
    #[default]
    BuyAndSell,
    Repair,
}

/// The shop's buttons (`m_BtnRepair`, `m_BtnRepairAll`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Button {
    Repair,
    RepairAll,
}

/// `CNewUINPCShop` and the global `BuyCost`.
#[derive(Resource, Default, Debug)]
pub struct NpcShop {
    /// `IsVisible(INTERFACE_NPCSHOP)`.
    pub open: bool,
    /// `m_bRepairShop`.
    pub repair_shop: bool,
    pub state: ShopState,
    /// `m_iTaxRate`.
    pub tax_rate: i32,
    /// `m_bSellingItem`: a `SendRequestSell` waits for its answer.
    pub selling: bool,
    /// `BuyCost` (WSclient.cpp:230): the price of the purchase waiting for its answer, 0 if none.
    buy_cost: i32,
    /// `m_bIsNPCShopOpen`: a purchase needs a release of the button after the shop opened.
    armed: bool,
    /// `GambleSystem::IsGambleShop` (window 0x22, WSclient.cpp:5949-5953).
    gamble: bool,
    /// The grid (`m_pNewInventoryCtrl`), by top-left cell.
    items: Vec<GridItem>,
    /// The store list the grid was filled from.
    source: Vec<(u8, [u8; 12])>,
    /// Grid `m_EventState` and `m_pToolTipItem` (index into `items`).
    hover: bool,
    tooltip: Option<usize>,
    /// A button held down.
    down: Option<Button>,
    /// [`NpcDialog::serial`] of the dialog shown.
    serial: u32,
    /// `AllRepairGold`, recomputed every frame of a repair shop (`Update` → `RepairAllGold`).
    pub repair_all_gold: i32,
}

impl NpcShop {
    /// Where an inventory repair would be paid (`ConvertRepairGold`, ZzzInventory.cpp:1187-1198).
    pub fn repair_place(&self, inventory_open: bool) -> RepairPlace {
        if self.open && self.repair_shop {
            RepairPlace::NpcShop
        } else if inventory_open && !self.open {
            RepairPlace::SelfRepair
        } else {
            RepairPlace::Other
        }
    }

    /// The mouse (640×480 screen) is over the window: a click there does not walk.
    pub fn blocks(&self, c: Vec2) -> bool {
        self.open && inside(c, POS.0, POS.1, W, H)
    }

    /// `OpenningProcess` (NewUINPCShop.cpp:418-428) with what `Show` sets: `m_bIsNPCShopOpen` only if
    /// the button is not held (the click that opened the shop must be released first).
    fn opening(&mut self, repair_shop: bool, gamble: bool, button_held: bool) {
        self.open = true;
        self.repair_shop = repair_shop;
        self.gamble = gamble;
        self.armed = !button_held;
    }

    /// `ClosingProcess` (NewUINPCShop.cpp:430-448), without `SendExitInventory` (npc.rs sends it).
    /// `BuyCost` is a global the original does not clear here.
    fn closing(&mut self) {
        *self = NpcShop { buy_cost: self.buy_cost, ..NpcShop::default() };
    }

    /// `InsertItem` for each store item (`AddItem(index, packet)`: the cell is `index % 8, index / 8`).
    fn fill(&mut self, store: &[(u8, [u8; 12])]) {
        self.source = store.to_vec();
        self.items = store
            .iter()
            .filter_map(|(s, b)| {
                let i = *s as usize;
                (i < COLUMNS * ROWS).then_some(())?;
                Some(GridItem { x: (i % COLUMNS) as u8, y: (i / COLUMNS) as u8, item: Item::from_bytes(b)? })
            })
            .collect();
        self.tooltip = None;
        self.hover = false;
    }

    /// The item covering a cell.
    fn owner(&self, data: &ItemData, x: usize, y: usize) -> Option<usize> {
        self.items.iter().position(|g| {
            let (w, h) = Inventory::size(data, &g.item);
            (g.x as usize..g.x as usize + w as usize).contains(&x) && (g.y as usize..g.y as usize + h as usize).contains(&y)
        })
    }

    /// `ToggleState` (NewUINPCShop.cpp:471-484): also the inventory's repair mode.
    fn toggle_state(&mut self, inv: &mut Inventory) {
        self.state = match self.state {
            ShopState::BuyAndSell => ShopState::Repair,
            ShopState::Repair => ShopState::BuyAndSell,
        };
        inv.set_repair_mode(self.state == ShopState::Repair);
    }

    /// `SendRequestBuy(index, cost)` (wsclientinline.h:722-732): nothing while a purchase is
    /// pending. Returns the packet to send.
    fn request_buy(&mut self, index: u8, cost: i32) -> Option<ClientMessage> {
        if self.buy_cost != 0 {
            return None;
        }
        self.buy_cost = cost;
        Some(ClientMessage::BuyItem { slot: index })
    }

    /// `SendRequestSell(index)` (wsclientinline.h:711-718): `SetSellingItem(true)`.
    fn request_sell(&mut self, slot: u8) -> ClientMessage {
        self.selling = true;
        ClientMessage::SellItem { slot }
    }

    /// What the shop does with the server's answers (`ReceiveBuy`, `ReceiveSell`; the items are
    /// inventory.rs'). Returns what else has to happen.
    fn apply(&mut self, msg: &ServerMessage) -> Answer {
        match msg {
            ServerMessage::ItemBought { .. } => {
                self.buy_cost = 0;
                Answer::None
            }
            ServerMessage::BuyFailed { closed } => {
                self.buy_cost = 0;
                // `Index` 0xFE: `HideAll` and `GlobalText[732]`; it also passes the `!= 255` test before,
                // so `SOUND_GET_ITEM01` plays (WSclient.cpp:6006-6026).
                if *closed { Answer::BuyRefusedClosed } else { Answer::None }
            }
            ServerMessage::SellResult { result, money } => {
                // `SetSellingItem(false)` is only on success in Main 5.2 (WSclient.cpp:6057), which
                // would leave a refused sale blocking the shop; MuMain clears it after every answer
                // (Network/Server/WSclient.cpp:6854, `UI::Shop::NpcSaleFinished`).
                self.selling = false;
                match result {
                    0 => Answer::None,
                    0xFF => Answer::SellRefused,
                    0xFE => Answer::SellRefusedClosed,
                    _ => Answer::Sold(*money),
                }
            }
            _ => Answer::None,
        }
    }
}

/// What a server answer asks of the rest of the game.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Answer {
    None,
    BuyRefusedClosed,
    SellRefused,
    SellRefusedClosed,
    /// `CharacterMachine->Gold = Data->Gold`.
    Sold(u32),
}

/// What the shop draws this frame, for inventory.rs (which draws it in its layers).
#[derive(Resource, Default)]
pub struct ShopDraw {
    pub back: Vec<Piece>,
    pub front: Vec<Piece>,
    pub views: Vec<ItemView>,
}

// --- Systems -----------------------------------------------------------------------------------

/// The shop follows npc.rs' dialog: it opens (with the inventory) and closes with it, and refills
/// its grid when the store list changes.
fn follow_dialog(
    dialog: Res<NpcDialog>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut shop: ResMut<NpcShop>,
    mut inv: ResMut<Inventory>,
) {
    let is_shop = dialog.kind() == Some(WindowKind::Shop);
    // Another shop window replacing this one (`HideAll` then `Show`): closes and opens again.
    if shop.open && (!is_shop || shop.serial != dialog.serial) {
        shop.closing();
        info!("tienda: se cierra");
    }
    if is_shop && !shop.open {
        shop.serial = dialog.serial;
        shop.opening(dialog.repair, dialog.window == Some(0x22), mouse.pressed(MouseButton::Left));
        // `ShowInterface(INTERFACE_INVENTORY)` (NewUISystem.cpp:663).
        inv.show();
        info!("tienda: se abre{}", if shop.repair_shop { " (repara)" } else { "" });
    }
    if shop.open && shop.source != dialog.store {
        shop.fill(&dialog.store);
    }
}

/// `ReceiveBuy` and `ReceiveSell` (WSclient.cpp:6003-6064): the messages, `HideAll` and the zen.
#[allow(clippy::too_many_arguments)]
fn server_messages(
    mut from: MessageReader<crate::net::FromServer>,
    mut shop: ResMut<NpcShop>,
    (mut dialog, mut net, mut inv): (ResMut<NpcDialog>, ResMut<crate::net::Net>, ResMut<Inventory>),
    (mut game_ui, mut char_info, mut move_window): (
        ResMut<crate::options::GameUi>,
        ResMut<crate::char_info::CharInfoWindow>,
        ResMut<crate::move_window::MoveWindow>,
    ),
    (mut chat, metrics): (ResMut<crate::chat::Chat>, Res<crate::chat::TextMetrics>),
    hero: Option<Res<crate::account::Hero>>,
    mut info: Option<ResMut<crate::account::HeroInfo>>,
    mut sound: ResMut<SoundQueue>,
) {
    let name = hero.as_ref().map_or(String::new(), |h| h.name.clone());
    for m in from.read() {
        let answer = shop.apply(&m.0);
        let mut hide_all = |dialog: &mut NpcDialog, net: &mut crate::net::Net, inv: &mut Inventory| {
            crate::npc::close_dialog(dialog, net, inv, "el servidor cierra la tienda");
            if inv.open {
                inv.toggle();
            }
            char_info.close();
            move_window.close();
            if !game_ui.modal() {
                game_ui.hide_all();
            }
        };
        // `g_pChatListBox->AddText(Hero->ID, GlobalText[...], TYPE_ERROR_MESSAGE)`.
        let mut error = |text: &str| {
            chat.add(&name, text, MsgType::Error, MsgType::All, &metrics);
        };
        match answer {
            Answer::None => {}
            Answer::BuyRefusedClosed => {
                sound.play("SOUND_GET_ITEM01");
                hide_all(&mut dialog, &mut net, &mut inv);
                error(CANNOT_BE_TRADED);
            }
            Answer::SellRefused => error(CANNOT_BE_SOLD),
            Answer::SellRefusedClosed => {
                hide_all(&mut dialog, &mut net, &mut inv);
                error(CANNOT_BE_SOLD);
            }
            Answer::Sold(money) => match info.as_mut() {
                Some(info) => info.0.money = money,
                None => inv.zen = money,
            },
        }
    }
}

/// `UpdateKeyEvent` (NewUINPCShop.cpp:166-199): `Shift+L` repairs everything, `L` switches the
/// repair state of a repair shop. (Esc is npc.rs'.)
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    game_ui: Res<crate::options::GameUi>,
    mut shop: ResMut<NpcShop>,
    mut inv: ResMut<Inventory>,
    mut net: ResMut<crate::net::Net>,
) {
    if !shop.open || game_ui.modal() || !keys.just_pressed(KeyCode::KeyL) {
        return;
    }
    if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        // `SendRequestRepair(255, 0)`, repair shop or not (the server decides).
        Inventory::request_repair(&mut net, 0xFF, false);
    } else if shop.repair_shop && inv.picked.is_none() {
        shop.toggle_state(&mut inv);
    }
}

/// `GetSquareIndexAtPt` of the shop grid.
fn square_at(p: Vec2) -> Option<usize> {
    if !inside(p, GRID.0, GRID.1, COLUMNS as f32 * SQUARE, ROWS as f32 * SQUARE) {
        return None;
    }
    let (x, y) = (((p.x - GRID.0) / SQUARE) as usize, ((p.y - GRID.1) / SQUARE) as usize);
    Some(y * COLUMNS + x)
}

/// `CNewUINPCShop::UpdateMouseEvent` (NewUINPCShop.cpp:88-164) with its grid's `UpdateProcess` and
/// `UpdateMouseEvent` (NewUIInventoryCtrl.cpp:785-860; the grid is locked: nothing is picked up).
#[allow(clippy::too_many_arguments)]
fn update_mouse(
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    data: Res<ItemData>,
    game_ui: Res<crate::options::GameUi>,
    info: Option<Res<crate::account::HeroInfo>>,
    (mut chat, metrics): (ResMut<crate::chat::Chat>, Res<crate::chat::TextMetrics>),
    mut shop: ResMut<NpcShop>,
    (mut dialog, mut inv, mut net): (ResMut<NpcDialog>, ResMut<Inventory>, ResMut<crate::net::Net>),
) {
    if !shop.open {
        return;
    }
    let Some(m) = windows.single().ok().and_then(crate::mainframe::cursor_virtual).filter(|_| !game_ui.modal()) else {
        return;
    };
    let m = m.floor();
    let (pressed, released, just_pressed) =
        (mouse.pressed(MouseButton::Left), mouse.just_released(MouseButton::Left), mouse.just_pressed(MouseButton::Left));
    let shop = &mut *shop;

    // The grid's `UpdateProcess` and `UpdateMouseEvent`: hover and tooltip only.
    let pointed = square_at(m).and_then(|s| shop.owner(&data, s % COLUMNS, s / COLUMNS));
    if pointed.is_none() {
        shop.hover = false;
        shop.tooltip = None;
    }
    if !shop.hover && !pressed && pointed.is_some() {
        shop.hover = true;
    } else if shop.hover && !pressed && inv.picked.is_none() && pointed != shop.tooltip {
        shop.tooltip = pointed;
    }

    let in_grid = square_at(m).is_some();
    // `InventoryProcess`: selling the held item.
    if released
        && in_grid
        && !shop.selling
        && let Some(p) = inv.picked
    {
        let attr = data.attr(&p.item);
        let zen = info.as_ref().map_or(inv.zen, |i| i.0.money) as i64;
        let value = attr.and_then(|a| price::item_value(a, &p.item, GoldType::Sell)).unwrap_or(0) as i64;
        let mut system = |text: &str, kind: MsgType| {
            chat.add("", text, kind, MsgType::All, &metrics);
        };
        if zen + value > MAX_ZEN {
            system(TOO_MUCH_ZEN, MsgType::System);
        } else if price::is_selling_ban(&p.item) {
            system(ITEMS_CANNOT_BE_TRADED, MsgType::Error);
            inv.backup_picked();
        } else if price::is_high_value_item(&p.item) {
            // TODO: `CHighValueItemCheckMsgBoxLayout` (no message boxes yet): its "Cancel".
            warn!("tienda: objeto valioso, falta la ventana de confirmación; vuelve al inventario");
            inv.backup_picked();
        } else if net.connected() {
            let packet = shop.request_sell(inventory::source_slot(p.from));
            info!("tienda: se pide vender {:?}", packet);
            net.send(packet);
        }
        return;
    }
    // Buying.
    if in_grid {
        if shop.armed && released && let Some(i) = pointed {
            let g = shop.items[i];
            if shop.gamble {
                // TODO: `CGambleBuyMsgBoxLayout` (no message boxes yet).
                warn!("tienda: la tienda de apuestas necesita su ventana de compra (aún no existe)");
            } else if net.connected() {
                let cost = data.attr(&g.item).and_then(|a| price::item_value(a, &g.item, GoldType::Buy)).unwrap_or(0);
                if let Some(packet) = shop.request_buy(g.y * COLUMNS as u8 + g.x, cost) {
                    info!("tienda: se pide comprar {:?} por {cost}", packet);
                    net.send(packet);
                }
            }
            return;
        } else if released {
            shop.armed = true;
            return;
        } else if just_pressed {
            return;
        }
    }

    // `BtnProcess`.
    if just_pressed && !shop.selling && inside(m, BTN_CLOSE.0, BTN_CLOSE.1, BTN_CLOSE.2, BTN_CLOSE.3) {
        crate::npc::close_dialog(&mut dialog, &mut net, &mut inv, "X de la tienda");
        return;
    }
    if shop.repair_shop {
        let over = |b: (f32, f32)| inside(m, b.0, b.1, BTN_W, BTN_H);
        let under = if over(BTN_REPAIR) {
            Some(Button::Repair)
        } else if over(BTN_REPAIR_ALL) {
            Some(Button::RepairAll)
        } else {
            None
        };
        if just_pressed {
            shop.down = under;
        }
        if !pressed {
            if released && shop.down.is_some() && shop.down == under {
                match under {
                    Some(Button::Repair) => shop.toggle_state(&mut inv),
                    Some(Button::RepairAll) => Inventory::request_repair(&mut net, 0xFF, false),
                    None => {}
                }
            }
            shop.down = None;
        }
    }
}

/// `Update` (`RepairAllGold`) and `Render` (NewUINPCShop.cpp:201-277) into [`ShopDraw`].
#[allow(clippy::too_many_arguments)]
fn prepare_draw(
    windows: Query<&Window>,
    data: Res<ItemData>,
    metrics: Res<crate::chat::TextMetrics>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    inv: Res<Inventory>,
    mut shop: ResMut<NpcShop>,
    mut draw: ResMut<ShopDraw>,
) {
    if !shop.open {
        if !draw.back.is_empty() || !draw.front.is_empty() || !draw.views.is_empty() {
            *draw = ShopDraw::default();
        }
        return;
    }
    if shop.repair_shop {
        let equipment = inv.equipment.iter().flatten().filter_map(|i| Some((data.attr(i)?, i)));
        let grid = inv.grid.iter().filter_map(|g| Some((data.attr(&g.item)?, &g.item)));
        let place = shop.repair_place(inv.open);
        shop.repair_all_gold = price::repair_all_gold(equipment, grid, place);
    }
    let m = windows.single().ok().and_then(crate::mainframe::cursor_virtual).map(|c| c.floor());
    let hero = mu_data::items::info::HeroView {
        class: hero.as_ref().map_or(1, |h| h.class.index() as u8),
        step: 1,
        level: hero.as_ref().map_or(1, |h| h.level),
        stats: info.as_ref().map(|i| {
            let i = &i.0;
            [i.strength as u32, i.agility as u32, i.vitality as u32, i.energy as u32, i.leadership as u32]
        }),
    };
    let (back, front, views) = shop_pieces(&shop, &inv, &data, &hero, m, &metrics);
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

/// The shop's background, tooltips and 3D items.
fn shop_pieces(
    shop: &NpcShop,
    inv: &Inventory,
    data: &ItemData,
    hero: &mu_data::items::info::HeroView,
    m: Option<Vec2>,
    metrics: &crate::chat::TextMetrics,
) -> (Vec<Piece>, Vec<Piece>, Vec<ItemView>) {
    let mut back = Vec::new();
    let (x, y) = POS;
    // `RenderFrame` (232-239).
    back.push(inventory::img(Tex::Back, x, y, W, H, 0.0));
    back.push(inventory::img(Tex::Top, x, y, W, 64.0, 0.0));
    back.push(inventory::img(Tex::Left, x, y + 64.0, 21.0, 320.0, 0.0));
    back.push(inventory::img(Tex::Right, x + W - 21.0, y + 64.0, 21.0, 320.0, 0.0));
    back.push(inventory::img(Tex::Bottom, x, y + H - 45.0, W, 45.0, 0.0));
    // `RenderTexts` (241-252): bold, (220, 220, 220), centered.
    let grey = [220.0 / 255.0; 3];
    back.push(inventory::text(x, y + 12.0, Some(W), TITLE, grey, true));
    back.push(inventory::text(x, y + 27.0, Some(W), &tax_text(shop.tax_rate), grey, true));
    if shop.repair_shop {
        // `RenderButton` (254-261).
        let over = |b: (f32, f32)| m.is_some_and(|m| inside(m, b.0, b.1, BTN_W, BTN_H));
        let mut tips = Vec::new();
        for (button, pos, tip) in [(Button::Repair, BTN_REPAIR, inventory::TIP_REPAIR), (Button::RepairAll, BTN_REPAIR_ALL, TIP_REPAIR_ALL)] {
            let frame = if shop.down == Some(button) && over(pos) { BTN_H } else { 0.0 };
            back.push(inventory::img(Tex::Repair, pos.0, pos.1, BTN_W, BTN_H, frame));
            if over(pos) {
                tips.push((tip, pos));
            }
        }
        // `RenderRepairMoney` (263-277).
        back.push(inventory::img(Tex::Money2, x + 10.0, y + 355.0, 170.0, 24.0, 0.0));
        back.push(inventory::text(x + 20.0, y + 362.0, None, REPAIR_ALL, [1.0, 220.0 / 255.0, 150.0 / 255.0], true));
        let gold = shop.repair_all_gold;
        back.push(inventory::text(x + 100.0, y + 362.0, None, &price::convert_gold(gold as i64), inventory::gold_color(gold as u32), true));
        for (s, (bx, by)) in tips {
            inventory::push_button_tip(&mut back, s, bx, by, metrics);
        }
    }
    // The grid (`m_pNewInventoryCtrl->Render`), with the held item's landing cells: red over the shop
    // for an item that cannot be sold (`LEM_ADD_LUCKYITEM`, NewUINPCShop.cpp:334-337).
    let occupied = |cx: usize, cy: usize| shop.owner(data, cx, cy).is_some();
    inventory::push_grid_frame(&mut back, GRID, COLUMNS, ROWS, &occupied);
    if let (Some(p), Some(m)) = (inv.picked, m) {
        let normal = if price::is_selling_ban(&p.item) { [1.0, 0.0, 0.0] } else { DROP_NORMAL };
        inventory::push_drop_overlay(&mut back, GRID, COLUMNS, ROWS, Inventory::size(data, &p.item), m, &occupied, normal);
    }

    // The tooltip (`RenderItemToolTip` with `TOOLTIP_TYPE_NPC_SHOP`, NewUIInventoryCtrl.cpp:1402-1436).
    let mut front = Vec::new();
    if let (Some(i), None) = (shop.tooltip, inv.picked)
        && let Some(g) = shop.items.get(i)
        && let Some(attr) = data.attr(&g.item)
    {
        let mana = |s: u16| data.skill_mana.as_ref().and_then(|m| m.get(s as usize)).map(|&m| m as u32);
        let mut lines = item_info(attr, &g.item, hero, &mana);
        if let Some(line) = price::price_line(attr, &g.item, true, shop.tax_rate) {
            price::add_price_line(&mut lines, line);
        }
        let sx = GRID.0 + g.x as f32 * SQUARE + attr.width as f32 * SQUARE / 2.0;
        let mut sy = GRID.1 + g.y as f32 * SQUARE;
        if attr.height == 1 {
            sy += SQUARE / 2.0;
        }
        inventory::push_tooltip(&mut front, &lines, sx.floor(), sy, metrics);
    }

    // `CNewUIInventoryCtrl::Render3D` (NewUIInventoryCtrl.cpp:1514-1530): spins when hovered.
    let free = inv.picked.is_none();
    let views = shop
        .items
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let (w, h) = Inventory::size(data, &g.item);
            let rect = [GRID.0 + g.x as f32 * SQUARE, GRID.1 + g.y as f32 * SQUARE, w as f32 * SQUARE, h as f32 * SQUARE];
            let hovered = free && m.is_some_and(|m| inside(m, rect[0], rect[1], rect[2], rect[3]));
            ItemView { key: 2000 + i as u32, item: g.item, rect, picked: false, hovered }
        })
        .collect();
    (back, front, views)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_purchase_at_a_time() {
        let mut shop = NpcShop::default();
        assert_eq!(shop.request_buy(17, 240), Some(ClientMessage::BuyItem { slot: 17 }));
        assert_eq!(shop.request_buy(18, 240), None, "BuyCost != 0");
        assert_eq!(shop.apply(&ServerMessage::ItemBought { slot: 12, item: [0; 12] }), Answer::None);
        assert_eq!(shop.request_buy(18, 240), Some(ClientMessage::BuyItem { slot: 18 }));
        // Refused: free again; with 0xFE the windows close.
        assert_eq!(shop.apply(&ServerMessage::BuyFailed { closed: false }), Answer::None);
        assert!(shop.request_buy(18, 240).is_some());
        assert_eq!(shop.apply(&ServerMessage::BuyFailed { closed: true }), Answer::BuyRefusedClosed);
        assert_eq!(shop.buy_cost, 0);
        // The global `BuyCost` survives the window closing.
        shop.buy_cost = 5;
        shop.closing();
        assert_eq!(shop.buy_cost, 5);
    }

    #[test]
    fn selling_waits_for_the_server() {
        let mut shop = NpcShop::default();
        assert_eq!(shop.request_sell(31), ClientMessage::SellItem { slot: 31 });
        assert!(shop.selling);
        assert_eq!(shop.apply(&ServerMessage::SellResult { result: 1, money: 1234 }), Answer::Sold(1234));
        assert!(!shop.selling);
        shop.request_sell(31);
        assert_eq!(shop.apply(&ServerMessage::SellResult { result: 0, money: 0 }), Answer::None);
        assert!(!shop.selling, "a refused sale does not block the shop (MuMain)");
        shop.request_sell(31);
        assert_eq!(shop.apply(&ServerMessage::SellResult { result: 0xFF, money: 0 }), Answer::SellRefused);
        assert_eq!(shop.apply(&ServerMessage::SellResult { result: 0xFE, money: 0 }), Answer::SellRefusedClosed);
    }

    #[test]
    fn sale_answer_moves_the_held_item() {
        let kris = Item { durability: 20, ..Item::new(0, 1) };
        let mut inv = Inventory::default();
        inv.picked = Some(inventory::Picked { item: kris, from: inventory::Source::Grid { x: 3, y: 2 } });
        assert_eq!(inventory::source_slot(inv.picked.unwrap().from), 31);
        // Refused: back to its cell.
        inv.apply(&ServerMessage::SellResult { result: 0, money: 0 });
        assert!(inv.picked.is_none());
        assert_eq!(inv.grid, vec![GridItem { x: 3, y: 2, item: kris }]);
        // Sold: gone.
        let g = inv.grid.remove(0);
        inv.picked = Some(inventory::Picked { item: g.item, from: inventory::Source::Grid { x: 3, y: 2 } });
        assert_eq!(inv.apply(&ServerMessage::SellResult { result: 1, money: 99 }), Some("SOUND_GET_ITEM01"));
        assert!(inv.picked.is_none() && inv.grid.is_empty());
        // Bought: into the slot the server says.
        inv.apply(&ServerMessage::ItemBought { slot: 12 + 8 + 1, item: kris.to_bytes() });
        assert_eq!(inv.grid, vec![GridItem { x: 1, y: 1, item: kris }]);
    }

    #[test]
    fn grid_and_state() {
        let mut shop = NpcShop::default();
        let kris = Item { durability: 20, ..Item::new(0, 1) };
        // Store slot 17 is cell (1, 2); slots past the 8×15 grid are dropped.
        shop.fill(&[(17, kris.to_bytes()), (120, kris.to_bytes())]);
        assert_eq!(shop.items, vec![GridItem { x: 1, y: 2, item: kris }]);
        assert_eq!(square_at(Vec2::new(GRID.0 + 1.0, GRID.1 + 1.0)), Some(0));
        assert_eq!(square_at(Vec2::new(GRID.0 + 159.0, GRID.1 + 299.0)), Some(119));
        assert_eq!(square_at(Vec2::new(GRID.0 + 160.0, GRID.1)), None);
        // The click that opened the shop must be released before buying.
        shop.opening(true, false, true);
        assert!(!shop.armed && shop.repair_shop);
        let mut inv = Inventory::default();
        shop.toggle_state(&mut inv);
        assert_eq!(shop.state, ShopState::Repair);
        assert!(inv.repair_mode);
        assert_eq!(shop.repair_place(true), RepairPlace::NpcShop);
        shop.toggle_state(&mut inv);
        assert!(!inv.repair_mode);
        shop.closing();
        assert!(!shop.open && shop.items.is_empty());
        assert_eq!(shop.repair_place(true), RepairPlace::SelfRepair);
    }
}
