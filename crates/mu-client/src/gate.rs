//! Walk-in gates: `CheckGate` (ZzzInterface.cpp:7023-7149) and the surrounding part of `MoveHero`
//! (`LoadingWorld`, ZzzInterface.cpp:7185-7195).
//!
//! - **Every tick** (25 per second) `LoadingWorld` decreases; at 9999998 the hero stops
//!   (`SetPlayerStop`). Above 30 the hero ignores the mouse and doesn't check gates.
//! - **Gate**: with `LoadingWorld` at 0, the first entry gate (`Flag` 1) of the map
//!   (`WorldActive`) whose `Gate.bmd` rectangle contains the hero's tile (`PositionX/Y`, edges
//!   included). The level is that gate's, 2/3 for MG, DL and RF (`gs->Level * 2 / 3`).
//!   - Gate 28 without the level: nothing (the original's `Success`).
//!   - Gates 62-65 (Icarus) without wings, Dinorant or Fenrir: `GlobalText[263]` and, if the level
//!     is also missing, `GlobalText[350]`. With no equipment yet, always. (The original repeats it every
//!     tick while standing on the gate; here it waits the same as for the level, 50 ticks.)
//!   - Gates 45-49 and 55-56 (Atlans) with Uniria (`GlobalText[261]`) and 62-65 with Uniria
//!     (`GlobalText[569]`): without equipment nothing is mounted, never happens.
//!   - Without the level: `LoadingWorld` = 50 and `GlobalText[350]` in chat (`TYPE_ERROR_MESSAGE`).
//!   - Otherwise: `SendRequestMagicTeleport(&bResult, gate, 0, 0)` (wsclientinline.h:655-675), which is
//!     not sent while already changing map (`g_bWhileMovingZone`) or less than 3 s after
//!     `SendRequestFinishLoading` (`g_dwLatestZoneMoving`); if sent, `LoadingWorld` = 9999999
//!     and `g_bWhileMovingZone` (except between Lorencia and Valley of Loren, which doesn't exist).
//! - **Reply** (`ReceiveTeleport`, WSclient.cpp:1698-1816): with a map change, `LoadingWorld`
//!   = 30 and, after loading, `SendRequestFinishLoading` ([`GateState::finish_loading`], move_window.rs).
//!   Also when entering the game (`ReceiveJoinMapServer`, WSclient.cpp:937). Without a map change
//!   (`Flag` 0, what OpenMU replies, and we do, if the server doesn't let you through) the original doesn't
//!   touch `LoadingWorld` and would leave the hero unable to move: here it also goes back to 30. For the
//!   same reason, if nothing arrives within 5 s it is released (a server that doesn't reply, e.g. with
//!   another `Gate.bmd`).

use bevy::prelude::*;
use mu_data::warps::Gate;
use mu_protocol::game::{ClientMessage, ServerMessage};

use crate::Args;
use crate::account::Class;
use crate::chat_log::MsgType;
use crate::scene::GameScene;

pub struct GatePlugin;

impl Plugin for GatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GateState>()
            .add_systems(Startup, load_gates)
            .add_systems(PreUpdate, net_messages.after(crate::net::NetSystems))
            .add_systems(Update, check_gate.before(crate::character::HeroInput).run_if(in_state(GameScene::Game)));
    }
}

/// `GlobalText[350]` and `[263]` (Text_eng.bmd).
const TEXT_LEVEL: &str = "Only characters over level %d can enter.";
const TEXT_ICARUS: &str = crate::chat_log::TEXT_ICARUS_NEEDS_WINGS;
/// `LoadingWorld` after requesting the gate.
const LOADING_SENT: u32 = 9_999_999;
/// `LoadingWorld` after changing map (or entering the game).
const LOADING_ARRIVED: u32 = 30;
/// `LoadingWorld` after falling short of the level.
const LOADING_REFUSED: u32 = 50;
/// What `SendRequestMagicTeleport` waits for after `SendRequestFinishLoading` (3000 ms).
const ZONE_MOVING_WAIT: f64 = 3.0;
/// Without a server reply within this time, the hero is released (not in the original).
const NO_REPLY: f64 = 5.0;

/// The original's global variables used by `CheckGate`.
#[derive(Resource, Default)]
pub struct GateState {
    /// `GateAttribute` (`Gate.bmd`).
    gates: Vec<Gate>,
    /// `LoadingWorld`.
    loading_world: u32,
    /// `g_bWhileMovingZone`.
    while_moving_zone: bool,
    /// `g_dwLatestZoneMoving` (seconds of `Time<Real>`).
    latest_zone_moving: Option<f64>,
    /// When the last gate was requested (for [`NO_REPLY`]).
    sent_at: Option<f64>,
    /// Accumulated tick fraction.
    acc: f32,
}

impl GateState {
    /// `LoadingWorld > 30`: the hero ignores the mouse.
    pub fn blocks_input(&self) -> bool {
        self.loading_world > LOADING_ARRIVED
    }

    /// `SendRequestFinishLoading` (wsclientinline.h:314-323): the new map has finished loading.
    pub fn finish_loading(&mut self, now: f64) {
        self.latest_zone_moving = Some(now);
        self.while_moving_zone = false;
    }
}

fn load_gates(args: Res<Args>, mut state: ResMut<GateState>) {
    match mu_data::warps::load_gates(&args.data) {
        Ok(g) => state.gates = g,
        Err(e) => warn!("sin Gate.bmd ({e}): las puertas no llevan a ningún sitio"),
    }
}

/// `ReceiveJoinMapServer` and `ReceiveTeleport`: `LoadingWorld = 30`.
fn net_messages(mut from: MessageReader<crate::net::FromServer>, mut state: ResMut<GateState>) {
    for m in from.read() {
        match m.0 {
            ServerMessage::CharacterInformation(_) => state.loading_world = LOADING_ARRIVED,
            ServerMessage::MapChanged { is_map_change, .. } => {
                if is_map_change || state.loading_world > LOADING_ARRIVED {
                    state.loading_world = LOADING_ARRIVED;
                }
                state.sent_at = None;
            }
            _ => {}
        }
    }
}

/// What `CheckGate` says for the hero on a tile.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// `SendRequestMagicTeleport(&bResult, gate, 0, 0)`.
    Send(u16),
    /// Level too low (`GlobalText[350]` with the level).
    Level(i32),
    /// Icarus without wings (`GlobalText[263]`, and `[350]` if the level is also missing).
    Wings(Option<i32>),
}

/// The level a gate requires (ZzzInterface.cpp:7044-7051): 2/3 for MG (`CLASS_DARK`), DL and RF.
fn gate_level(class: Class, level: u16) -> i32 {
    if matches!(class, Class::Gladiator | Class::Lord | Class::Fighter) { level as i32 * 2 / 3 } else { level as i32 }
}

/// The `CheckGate` loop (without `LoadingWorld`): the first entry gate of `map` stepped on at
/// `(x, y)` and what to do.
fn check(gates: &[Gate], map: u8, (x, y): (i32, i32), class: Class, hero_level: i32) -> Option<Outcome> {
    for (i, gs) in gates.iter().enumerate() {
        if gs.flag != 1 || gs.map != map {
            continue;
        }
        // Corners not sorted, like the original (`x >= x1 && x <= x2`).
        if !(x >= gs.x1 as i32 && y >= gs.y1 as i32 && x <= gs.x2 as i32 && y <= gs.y2 as i32) {
            continue;
        }
        let level = gate_level(class, gs.level);
        // Gate 28 (Devias → Lost Tower) without the level does nothing.
        if i == 28 && hero_level < level {
            return None;
        }
        // Atlans with Uniria (`GlobalText[261]`): without equipment, never.
        if (62..=65).contains(&i) {
            // No wings, Dinorant or Fenrir (no equipment yet).
            return Some(Outcome::Wings((hero_level < level).then_some(level)));
        }
        if hero_level < level {
            return Some(Outcome::Level(level));
        }
        return Some(Outcome::Send(i as u16));
    }
    None
}

/// `MoveHero` (the `LoadingWorld` part) + `CheckGate`, once per tick.
#[allow(clippy::too_many_arguments)]
fn check_gate(
    time: Res<Time>,
    real: Res<Time<Real>>,
    args: Res<Args>,
    hero: Option<Res<crate::account::Hero>>,
    mut state: ResMut<GateState>,
    mut players: Query<&mut crate::character::Player>,
    mut chat: ResMut<crate::chat::Chat>,
    metrics: Res<crate::chat::TextMetrics>,
    mut net: ResMut<crate::net::Net>,
) {
    let Ok(mut p) = players.single_mut() else { return };
    let now = real.elapsed_secs_f64();
    let state = &mut *state;
    if state.sent_at.is_some_and(|t| now - t > NO_REPLY) {
        warn!("puerta: el servidor no contesta, se suelta al personaje");
        state.sent_at = None;
        state.loading_world = LOADING_ARRIVED;
    }
    state.acc = (state.acc + time.delta_secs() * crate::fx::sim::TICKS_PER_SECOND).min(3.0);
    while state.acc >= 1.0 {
        state.acc -= 1.0;
        if state.loading_world > 0 {
            state.loading_world -= 1;
            // `SetPlayerStop` (at 29 too, but that only changes the animation for one tick: it keeps walking).
            if state.loading_world == LOADING_SENT - 1 {
                p.path.clear();
                p.operate = None;
            }
        }
        // `LoadingWorld > 30`: `return` before `CheckGate`; and `CheckGate` requires 0.
        if state.loading_world != 0 || !net.connected() {
            continue;
        }
        let Some(hero) = hero.as_ref() else { continue };
        let map = (args.world as i32 - 1).clamp(0, 255) as u8;
        let tile = (p.pos.x.floor() as i32, p.pos.y.floor() as i32);
        let Some(outcome) = check(&state.gates, map, tile, hero.class, hero.level as i32) else { continue };
        let mut error = |text: &str| {
            chat.add("", text, MsgType::Error, MsgType::All, &metrics);
        };
        match outcome {
            Outcome::Send(gate) => {
                // `SendRequestMagicTeleport`: not sent while changing map or until 3 s later.
                let busy = state.while_moving_zone || state.latest_zone_moving.is_some_and(|t| now - t < ZONE_MOVING_WAIT);
                if busy {
                    state.while_moving_zone = false;
                    continue;
                }
                info!("puerta {gate} en ({}, {}): se pide cruzar", tile.0, tile.1);
                net.send(ClientMessage::EnterGate { gate, x: 0, y: 0 });
                state.loading_world = LOADING_SENT;
                state.while_moving_zone = true;
                state.sent_at = Some(now);
            }
            Outcome::Level(level) => {
                state.loading_world = LOADING_REFUSED;
                error(&TEXT_LEVEL.replace("%d", &level.to_string()));
            }
            Outcome::Wings(level) => {
                state.loading_world = LOADING_REFUSED;
                error(TEXT_ICARUS);
                if let Some(level) = level {
                    error(&TEXT_LEVEL.replace("%d", &level.to_string()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(flag: u8, map: u8, (x1, y1, x2, y2): (u8, u8, u8, u8), target: u16, level: u16) -> Gate {
        Gate { flag, map, x1, y1, x2, y2, target, angle: 0, level, max_level: 0 }
    }

    /// The Lorencia ↔ Dungeon gates from `Gate.bmd` (1 → 2 and 3 → 4) and one in Icarus.
    fn gates() -> Vec<Gate> {
        let mut g = vec![Gate::default(); 70];
        g[1] = gate(1, 0, (121, 232, 123, 233), 2, 20);
        g[2] = gate(2, 1, (107, 247, 110, 247), 0, 20);
        g[3] = gate(1, 1, (108, 248, 109, 248), 4, 0);
        g[4] = gate(2, 0, (121, 231, 123, 231), 0, 0);
        g[28] = gate(1, 2, (2, 248, 3, 249), 29, 40);
        g[62] = gate(1, 2, (10, 10, 11, 11), 63, 170);
        g
    }

    #[test]
    fn pisar_puertas() {
        let g = gates();
        assert_eq!(check(&g, 0, (122, 232), Class::Knight, 20), Some(Outcome::Send(1)));
        assert_eq!(check(&g, 0, (123, 233), Class::Knight, 50), Some(Outcome::Send(1)), "con los bordes");
        assert_eq!(check(&g, 0, (122, 231), Class::Knight, 50), None, "la de llegada no lleva a ningún sitio");
        assert_eq!(check(&g, 0, (124, 232), Class::Knight, 50), None);
        assert_eq!(check(&g, 1, (122, 232), Class::Knight, 50), None, "otro mapa");
        assert_eq!(check(&g, 0, (122, 232), Class::Knight, 19), Some(Outcome::Level(20)));
        // 20 * 2 / 3 = 13 for MG, DL and RF.
        assert_eq!(check(&g, 0, (122, 232), Class::Lord, 13), Some(Outcome::Send(1)));
        assert_eq!(check(&g, 0, (122, 232), Class::Gladiator, 12), Some(Outcome::Level(13)));
        assert_eq!(check(&g, 0, (122, 232), Class::Wizard, 13), Some(Outcome::Level(20)));
        // Back, without the level.
        assert_eq!(check(&g, 1, (108, 248), Class::Knight, 1), Some(Outcome::Send(3)));
        // Gate 28 without the level says nothing.
        assert_eq!(check(&g, 2, (2, 248), Class::Knight, 39), None);
        assert_eq!(check(&g, 2, (2, 248), Class::Knight, 40), Some(Outcome::Send(28)));
        // Icarus without wings: always the warning, plus the level one if missing.
        assert_eq!(check(&g, 2, (10, 10), Class::Knight, 400), Some(Outcome::Wings(None)));
        assert_eq!(check(&g, 2, (10, 10), Class::Knight, 100), Some(Outcome::Wings(Some(170))));
    }

    #[test]
    fn puertas_de_dist() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dist/cliente/Data");
        let Ok(g) = mu_data::warps::load_gates(&data) else { return };
        assert_eq!(check(&g, 0, (122, 233), Class::Knight, 20), Some(Outcome::Send(1)));
        assert_eq!(check(&g, 1, (109, 248), Class::Knight, 1), Some(Outcome::Send(3)));
        assert_eq!(TEXT_LEVEL.replace("%d", "20"), "Only characters over level 20 can enter.");
    }
}
