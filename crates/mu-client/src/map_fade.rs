//! Fade to black and back when the server moves the hero to another map.
//!
//! Not in the original: MuMain's `ReceiveTeleport` (Network/Server/WSclient.cpp:2312) calls `LoadWorld` at
//! once and the map simply pops in (only the hero fades in, `o->Alpha = 0`). The fade was asked
//! for on purpose, to make the change less abrupt; its timings are ours.
//!
//! - **Out**: on `MapChanged` to another map (move_window.rs) the screen goes black over
//!   [`OUT_SECS`] while the hero stands still and the mouse is taken by the fade
//!   ([`MapFade::blocks`], like being over the interface). Then the new map starts loading.
//! - **Black**: while it loads (the loading screen of a map change is black too, scene.rs).
//! - **In**: back in game, the black clears over [`IN_SECS`].

use bevy::prelude::*;

use crate::scene::GameScene;

/// Fading out (seconds). Ours, not from a reference.
pub const OUT_SECS: f64 = 0.25;
/// Fading in (seconds). Ours, not from a reference.
pub const IN_SECS: f64 = 0.4;

pub struct MapFadePlugin;

impl Plugin for MapFadePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapFade>()
            .add_systems(OnEnter(GameScene::Game), fade_in)
            .add_systems(Update, (start_load, draw).chain());
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
enum Phase {
    #[default]
    None,
    /// Going black since `since` (real seconds), then loading `world`.
    Out { since: f64, world: u16 },
    /// Black, loading.
    Black,
    /// Clearing since `since`.
    In { since: f64 },
}

#[derive(Resource, Default)]
pub struct MapFade {
    phase: Phase,
}

impl MapFade {
    /// Starts going black before loading `world` (another `MapChanged` meanwhile only changes the
    /// map to load).
    pub fn out(&mut self, world: u16, now: f64) {
        self.phase = match self.phase {
            Phase::Out { since, .. } => Phase::Out { since, world },
            _ => Phase::Out { since: now, world },
        };
    }

    /// The mouse does nothing in game while the screen goes black.
    pub fn blocks(&self) -> bool {
        matches!(self.phase, Phase::Out { .. })
    }

    /// How black the screen is at `now` (0 = clear, 1 = black).
    fn alpha(&self, now: f64) -> f32 {
        match self.phase {
            Phase::None => 0.0,
            Phase::Out { since, .. } => ((now - since) / OUT_SECS).clamp(0.0, 1.0) as f32,
            Phase::Black => 1.0,
            Phase::In { since } => (1.0 - (now - since) / IN_SECS).clamp(0.0, 1.0) as f32,
        }
    }

    /// The step at `now`: `Some(world)` once fully black (time to load it); clearing ends here.
    fn step(&mut self, now: f64) -> Option<u16> {
        match self.phase {
            Phase::Out { since, world } if now - since >= OUT_SECS => {
                self.phase = Phase::Black;
                Some(world)
            }
            Phase::In { since } if now - since >= IN_SECS => {
                self.phase = Phase::None;
                None
            }
            _ => None,
        }
    }
}

#[derive(Component)]
struct FadeOverlay;

fn fade_in(mut fade: ResMut<MapFade>, real: Res<Time<Real>>) {
    if fade.phase == Phase::Black {
        fade.phase = Phase::In { since: real.elapsed_secs_f64() };
    }
}

/// Fully black: `LoadWorld` of the new map (what move_window.rs did right away before).
fn start_load(
    mut fade: ResMut<MapFade>,
    real: Res<Time<Real>>,
    scene: Res<State<GameScene>>,
    mut args: ResMut<crate::Args>,
    mut load: ResMut<crate::scene::LoadStep>,
    mut next: ResMut<NextState<GameScene>>,
    mut pending: ResMut<crate::move_window::MapChangePending>,
) {
    // Left the game meanwhile (disconnected, back to character select): nothing to fade.
    if fade.blocks() && *scene.get() != GameScene::Game {
        fade.phase = Phase::None;
        return;
    }
    if let Some(world) = fade.step(real.elapsed_secs_f64()) {
        crate::scene::change_map(&mut args, &mut load, world);
        next.set(GameScene::Loading);
        pending.0 = true;
    }
}

/// The black layer over everything but the cursor (the loading screens are at 100).
fn draw(mut commands: Commands, fade: Res<MapFade>, real: Res<Time<Real>>, mut overlay: Query<(Entity, &mut BackgroundColor), With<FadeOverlay>>) {
    let alpha = fade.alpha(real.elapsed_secs_f64());
    match overlay.single_mut() {
        Ok((e, _)) if alpha <= 0.0 => commands.entity(e).despawn(),
        Ok((_, mut color)) => color.0 = Color::BLACK.with_alpha(alpha),
        Err(_) if alpha > 0.0 => {
            commands.spawn((
                Name::new("Fundido de mapa"),
                FadeOverlay,
                Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
                BackgroundColor(Color::BLACK.with_alpha(alpha)),
                GlobalZIndex(99),
                Pickable::IGNORE,
            ));
        }
        Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_black_in() {
        let mut f = MapFade::default();
        assert_eq!(f.alpha(0.0), 0.0);
        f.out(34, 10.0);
        assert!(f.blocks());
        assert!((f.alpha(10.125) - 0.5).abs() < 1e-3);
        // Another map change while going black: same start, new map.
        f.out(5, 10.1);
        assert_eq!(f.step(10.2), None);
        assert_eq!(f.step(10.25), Some(5));
        assert!(!f.blocks());
        assert_eq!(f.alpha(11.0), 1.0);
        f.phase = Phase::In { since: 20.0 };
        assert!((f.alpha(20.2) - 0.5).abs() < 1e-3);
        assert_eq!(f.step(20.3), None);
        assert_ne!(f.phase, Phase::None);
        assert_eq!(f.step(20.41), None);
        assert_eq!(f.phase, Phase::None);
        assert_eq!(f.alpha(20.41), 0.0);
    }
}
