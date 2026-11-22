//! Puertas a pie: `CheckGate` (ZzzInterface.cpp:7023-7149) y lo de `MoveHero` que la rodea
//! (`LoadingWorld`, ZzzInterface.cpp:7185-7195).
//!
//! - **Cada tick** (25 por segundo) baja `LoadingWorld`; a 9999998 el héroe se para
//!   (`SetPlayerStop`). Con más de 30 el héroe no atiende al ratón ni mira las puertas.
//! - **Puerta**: con `LoadingWorld` a 0, la primera puerta de entrada (`Flag` 1) del mapa
//!   (`WorldActive`) cuyo rectángulo de `Gate.bmd` tiene la casilla del héroe (`PositionX/Y`, con
//!   los bordes). El nivel es el de esa puerta, 2/3 para MG, DL y RF (`gs->Level * 2 / 3`).
//!   - Puerta 28 sin nivel: nada (el `Success` del original).
//!   - Puertas 62-65 (Icarus) sin alas, Dinorant ni Fenrir: `GlobalText[263]` y, si además le
//!     falta nivel, `GlobalText[350]`. Sin equipo todavía, siempre. (El original lo repite en cada
//!     tick mientras pisa la puerta; aquí espera lo mismo que con el nivel, 50 ticks.)
//!   - Puertas 45-49 y 55-56 (Atlans) con Uniria (`GlobalText[261]`) y 62-65 con Uniria
//!     (`GlobalText[569]`): sin equipo no se monta nada, no pasa nunca.
//!   - Sin nivel: `LoadingWorld` = 50 y `GlobalText[350]` en el chat (`TYPE_ERROR_MESSAGE`).
//!   - Si no: `SendRequestMagicTeleport(&bResult, puerta, 0, 0)` (wsclientinline.h:655-675), que no
//!     sale si ya se está cambiando de mapa (`g_bWhileMovingZone`) o hace menos de 3 s de
//!     `SendRequestFinishLoading` (`g_dwLatestZoneMoving`); si sale, `LoadingWorld` = 9999999
//!     y `g_bWhileMovingZone` (salvo entre Lorencia y Valley of Loren, que no hay).
//! - **Respuesta** (`ReceiveTeleport`, WSclient.cpp:1698-1816): con cambio de mapa, `LoadingWorld`
//!   = 30 y, al cargar, `SendRequestFinishLoading` ([`GateState::finish_loading`], move_window.rs).
//!   Al entrar al juego también (`ReceiveJoinMapServer`, WSclient.cpp:937). Sin cambio de mapa
//!   (`Flag` 0, lo que contesta OpenMU, y nosotros, si el servidor no deja pasar) el original no
//!   toca `LoadingWorld` y se quedaría sin poder moverse: aquí también vuelve a 30. Por lo mismo, si
//!   en 5 s no llega nada se suelta (un servidor que no contesta, p. ej. con otro `Gate.bmd`).

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

/// `GlobalText[350]` y `[263]` (Text_eng.bmd).
const TEXT_LEVEL: &str = "Only characters over level %d can enter.";
const TEXT_ICARUS: &str = crate::chat_log::TEXT_ICARUS_NEEDS_WINGS;
/// `LoadingWorld` tras pedir la puerta.
const LOADING_SENT: u32 = 9_999_999;
/// `LoadingWorld` tras cambiar de mapa (o entrar al juego).
const LOADING_ARRIVED: u32 = 30;
/// `LoadingWorld` tras no llegar al nivel.
const LOADING_REFUSED: u32 = 50;
/// Lo que espera `SendRequestMagicTeleport` desde `SendRequestFinishLoading` (3000 ms).
const ZONE_MOVING_WAIT: f64 = 3.0;
/// Sin respuesta del servidor en este tiempo, el héroe se suelta (no está en el original).
const NO_REPLY: f64 = 5.0;

/// Las variables globales del original que usa `CheckGate`.
#[derive(Resource, Default)]
pub struct GateState {
    /// `GateAttribute` (`Gate.bmd`).
    gates: Vec<Gate>,
    /// `LoadingWorld`.
    loading_world: u32,
    /// `g_bWhileMovingZone`.
    while_moving_zone: bool,
    /// `g_dwLatestZoneMoving` (segundos de `Time<Real>`).
    latest_zone_moving: Option<f64>,
    /// Cuándo se pidió la última puerta (para [`NO_REPLY`]).
    sent_at: Option<f64>,
    /// Fracción de tick acumulada.
    acc: f32,
}

impl GateState {
    /// `LoadingWorld > 30`: el héroe no atiende al ratón.
    pub fn blocks_input(&self) -> bool {
        self.loading_world > LOADING_ARRIVED
    }

    /// `SendRequestFinishLoading` (wsclientinline.h:314-323): ya ha cargado el mapa nuevo.
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

/// `ReceiveJoinMapServer` y `ReceiveTeleport`: `LoadingWorld = 30`.
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

/// Lo que dice `CheckGate` para el héroe en una casilla.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// `SendRequestMagicTeleport(&bResult, puerta, 0, 0)`.
    Send(u16),
    /// Sin nivel (`GlobalText[350]` con el nivel).
    Level(i32),
    /// Icarus sin alas (`GlobalText[263]`, y `[350]` si además le falta nivel).
    Wings(Option<i32>),
}

/// El nivel que pide una puerta (ZzzInterface.cpp:7044-7051): 2/3 para MG (`CLASS_DARK`), DL y RF.
fn gate_level(class: Class, level: u16) -> i32 {
    if matches!(class, Class::Gladiator | Class::Lord | Class::Fighter) { level as i32 * 2 / 3 } else { level as i32 }
}

/// El bucle de `CheckGate` (sin `LoadingWorld`): la primera puerta de entrada de `map` que pisa
/// `(x, y)` y qué hacer.
fn check(gates: &[Gate], map: u8, (x, y): (i32, i32), class: Class, hero_level: i32) -> Option<Outcome> {
    for (i, gs) in gates.iter().enumerate() {
        if gs.flag != 1 || gs.map != map {
            continue;
        }
        // Sin ordenar las esquinas, como el original (`x >= x1 && x <= x2`).
        if !(x >= gs.x1 as i32 && y >= gs.y1 as i32 && x <= gs.x2 as i32 && y <= gs.y2 as i32) {
            continue;
        }
        let level = gate_level(class, gs.level);
        // La 28 (Devias → Lost Tower) sin nivel no hace nada.
        if i == 28 && hero_level < level {
            return None;
        }
        // Atlans con Uniria (`GlobalText[261]`): sin equipo, nunca.
        if (62..=65).contains(&i) {
            // Sin alas, Dinorant ni Fenrir (no hay equipo todavía).
            return Some(Outcome::Wings((hero_level < level).then_some(level)));
        }
        if hero_level < level {
            return Some(Outcome::Level(level));
        }
        return Some(Outcome::Send(i as u16));
    }
    None
}

/// `MoveHero` (lo de `LoadingWorld`) + `CheckGate`, una vez por tick.
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
            // `SetPlayerStop` (a 29 también, pero eso solo cambia la animación un tick: sigue caminando).
            if state.loading_world == LOADING_SENT - 1 {
                p.path.clear();
                p.operate = None;
            }
        }
        // `LoadingWorld > 30`: `return` antes de `CheckGate`; y `CheckGate` pide 0.
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
                // `SendRequestMagicTeleport`: no sale cambiando de mapa ni hasta 3 s después.
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

    /// Las puertas de Lorencia ↔ Dungeon de `Gate.bmd` (1 → 2 y 3 → 4) y una de Icarus.
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
        // 20 * 2 / 3 = 13 para MG, DL y RF.
        assert_eq!(check(&g, 0, (122, 232), Class::Lord, 13), Some(Outcome::Send(1)));
        assert_eq!(check(&g, 0, (122, 232), Class::Gladiator, 12), Some(Outcome::Level(13)));
        assert_eq!(check(&g, 0, (122, 232), Class::Wizard, 13), Some(Outcome::Level(20)));
        // De vuelta, sin nivel.
        assert_eq!(check(&g, 1, (108, 248), Class::Knight, 1), Some(Outcome::Send(3)));
        // La 28 sin nivel no dice nada.
        assert_eq!(check(&g, 2, (2, 248), Class::Knight, 39), None);
        assert_eq!(check(&g, 2, (2, 248), Class::Knight, 40), Some(Outcome::Send(28)));
        // Icarus sin alas: siempre el aviso, y el del nivel si falta.
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
