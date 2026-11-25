//! Map objects where the character sits, leans or heals (`OPERATE`, `CreateOperate`
//! in ZzzObject.cpp ≈4551 and each map's `CreateObject`), and what is under the mouse for the
//! cursor (`SelectOperate`, `DontMove`).
//!
//! - Each map marks some object types (benches, chairs, logs, invisible pose boxes...).
//! - `SelectOperate` (ZzzInterface.cpp ≈7815): the mouse ray against each one's box (the
//!   object position plus `BoundingBoxMin..Max`, unrotated and unscaled; by default
//!   (-40,-40,0)..(40,40,80)). The first hit is highlighted (`Light` = 1.5) and the rest stay
//!   at 0.2 (added to the terrain light, `VectorAdd(Light, o->Light, BodyLight)`).
//! - On click the character walks to its tile and, on arrival, `Action` with `MOVEMENT_OPERATE`
//!   (ZzzInterface.cpp ≈3587) sets the sitting, pose or healing animation.

use bevy::prelude::*;

use crate::model_light::ModelMaterial;
use crate::terrain::MU_SCALE;

pub struct OperatePlugin;

impl Plugin for OperatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Operates>().init_resource::<Pointer>().add_systems(
            Update,
            select_operate
                .after(crate::names::SelectObjects)
                .before(crate::character::HeroInput)
                .run_if(in_state(crate::scene::GameScene::Game)),
        );
    }
}

/// What the character does on arrival.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    /// `PLAYER_SIT1`.
    Sit,
    /// `PLAYER_POSE1`: leaning.
    Pose,
    /// `PLAYER_HEALING1` (the Noria fountain).
    Healing,
    /// Can be pointed at but does nothing (the Valley of Loren throne, only for the master of the
    /// guild that holds the castle).
    Nothing,
}

/// Rule for an object type on a map.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub act: Act,
    /// `Hero->Object.Angle[2] = TargetAngle`: faces where the object faces.
    pub face: bool,
    /// `CursorLeanAgainst` cursor (instead of `CursorSitDown`), see `RenderCursor`.
    pub lean_cursor: bool,
    /// `BoundingBoxMax` (the minimum is always (-40, -40, 0)).
    pub bbox_max: Vec3,
}

const BOX: Vec3 = Vec3::new(40.0, 40.0, 80.0);
/// Pose boxes: `Vector(40, 40, 160, BoundingBoxMax)`.
const TALL: Vec3 = Vec3::new(40.0, 40.0, 160.0);

/// Is `kind` a sit/lean object on map `world` (folder number, `WorldActive` + 1)?
pub fn rule(world: u16, kind: i16) -> Option<Rule> {
    use crate::fx::tables::ids::{MODEL_FURNITURE01, MODEL_POSE_BOX, MODEL_TREE01};
    let r = |act, face, lean_cursor, bbox_max| Some(Rule { act, face, lean_cursor, bbox_max });
    let k = kind as i32;
    match world {
        // Lorencia.
        1 if k == MODEL_POSE_BOX => r(Act::Pose, true, true, TALL),
        1 if k == MODEL_TREE01 + 6 => r(Act::Sit, false, false, BOX),
        1 if k == MODEL_FURNITURE01 + 5 => r(Act::Sit, true, false, BOX),
        1 if k == MODEL_FURNITURE01 + 6 => r(Act::Sit, false, false, BOX),
        // Dungeon.
        2 => match k {
            60 => r(Act::Pose, true, true, TALL),
            59 => r(Act::Sit, false, false, BOX),
            _ => None,
        },
        // Devias.
        3 => match k {
            91 => r(Act::Pose, true, true, TALL),
            22 | 25 | 40 | 55 => r(Act::Sit, true, false, BOX),
            45 | 73 => r(Act::Sit, false, false, BOX),
            _ => None,
        },
        // Noria: the fountain heals (and uses the lean cursor).
        4 => match k {
            8 => r(Act::Sit, false, false, BOX),
            38 => r(Act::Healing, true, true, BOX),
            _ => None,
        },
        // Atlans.
        8 if k == 39 => r(Act::Pose, true, false, BOX),
        // Tarkan.
        9 if k == 78 => r(Act::Sit, false, false, BOX),
        // Valley of Loren (castle): 77 is the guild master's throne.
        31 => match k {
            77 => r(Act::Nothing, true, false, BOX),
            84 => r(Act::Sit, true, false, BOX),
            _ => None,
        },
        // Kanturu (ruins, `M38Kanturu2nd`).
        40 if k == 3 => r(Act::Sit, false, false, BOX),
        // Elbeland (`GMNewTown`).
        52 if k == 103 => r(Act::Sit, true, false, BOX),
        // Loren Market (`GMUnitedMarketPlace`): 100×100×160 pose box.
        80 if k == 67 => r(Act::Pose, true, false, Vec3::new(100.0, 100.0, 160.0)),
        // `GMUnitedMarketPlace::CreateObject` does not check the map: in every map that falls into the
        // `default` of `CreateObject` (all but Lorencia, Dungeon, Devias, Noria, Atlans, Tarkan and the
        // login, ZzzObject.cpp:4676-4912) type 67 is also a 100×100×160 box. There `Action` has no
        // case: the character walks to it, stops and `SOUND_DROP_ITEM01` plays.
        // Chaos Castle never reaches the chain: `CreateChaosCastleObject` returns earlier.
        w if k == 67 && !matches!(w, 1 | 2 | 3 | 4 | 8 | 9 | 74 | 75 | 19..=24 | 54) => {
            r(Act::Nothing, false, false, Vec3::new(100.0, 100.0, 160.0))
        }
        _ => None,
    }
}

/// A sit object of the loaded map.
pub struct Operate {
    pub rule: Rule,
    /// MU position.
    pub position: Vec3,
    /// `Angle[2]` in degrees.
    pub angle: f32,
    /// Its own lit mesh materials (for highlighting) and the terrain light they have.
    pub materials: Vec<Handle<ModelMaterial>>,
    pub light: Vec3,
    /// Current `o->Light` (0 until the mouse moves over the game; then 0.2 or 1.5).
    pub glow: f32,
}

impl Operate {
    /// Object tile (`Position / TERRAIN_SCALE`).
    pub fn tile(&self) -> (i32, i32) {
        ((self.position.x / MU_SCALE) as i32, (self.position.y / MU_SCALE) as i32)
    }
}

#[derive(Resource, Default)]
pub struct Operates(pub Vec<Operate>);

/// What is under the mouse, for the cursor and the click.
#[derive(Resource, Default)]
pub struct Pointer {
    /// `SelectedOperate`.
    pub operate: Option<usize>,
    /// `DontMove`: the last ground click was on a tile that cannot be reached.
    pub dont_move: bool,
}

impl Pointer {
    /// Cursor of the pointed object: `Some(true)` lean, `Some(false)` sit.
    pub fn operate_cursor(&self, operates: &Operates) -> Option<bool> {
        self.operate.and_then(|i| operates.0.get(i)).map(|o| o.rule.lean_cursor)
    }
}

/// On map change.
pub fn reset(mut operates: ResMut<Operates>, mut pointer: ResMut<Pointer>) {
    operates.0.clear();
    *pointer = Pointer::default();
}

/// `o->Light` of the unpointed and the pointed object.
const GLOW_IDLE: f32 = 0.2;
const GLOW_SELECTED: f32 = 1.5;

/// Mouse ray in MU coordinates (origin, direction), if the mouse is in the window.
pub fn mouse_ray_mu(window: &Window, camera: &Camera, cam_tf: &GlobalTransform) -> Option<(Vec3, Vec3)> {
    let ray = camera.viewport_to_world(cam_tf, window.cursor_position()?).ok()?;
    let to_mu = |v: Vec3| Vec3::new(v.x, -v.z, v.y);
    Some((to_mu(ray.origin) * MU_SCALE, to_mu(*ray.direction)))
}

/// `SelectObjects` → `SelectOperate`: only with the mouse in the game area (above the bar,
/// `CheckMouseIn(0, 0, width, 429)`) and outside the windows.
fn select_operate(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<IsDefaultUiCamera>>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    hovered: Res<crate::names::Hovered>,
    mut operates: ResMut<Operates>,
    mut pointer: ResMut<Pointer>,
    mut materials: ResMut<Assets<ModelMaterial>>,
) {
    pointer.operate = None;
    let (Ok(window), Ok((camera, cam_tf))) = (windows.single(), cameras.single()) else { return };
    let in_area = crate::mainframe::cursor_virtual(window).is_some_and(|c| c.y < 429.0);
    // With a character under the mouse `SelectOperate` is not called: the lights stay as they were.
    if over_ui.0 || !in_area || hovered.any() || hovered.item.is_some() {
        return;
    }
    let Some((origin, dir)) = mouse_ray_mu(window, camera, cam_tf) else { return };
    let hit = operates.0.iter().position(|o| {
        let min = o.position + Vec3::new(-40.0, -40.0, 0.0);
        let max = o.position + o.rule.bbox_max;
        ray_hits_box(origin, dir, min, max)
    });
    pointer.operate = hit;
    for (i, o) in operates.0.iter_mut().enumerate() {
        let glow = if Some(i) == hit { GLOW_SELECTED } else { GLOW_IDLE };
        if o.glow == glow {
            continue;
        }
        o.glow = glow;
        let light = (o.light + Vec3::splat(glow)).extend(1.0);
        for h in &o.materials {
            if let Some(mut m) = materials.get_mut(h) {
                m.extension.light = light;
            }
        }
    }
}

/// `CollisionDetectLineToOBB` with an axis-aligned box (slab method).
pub(crate) fn ray_hits_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> bool {
    let (mut t0, mut t1) = (0.0f32, f32::INFINITY);
    for a in 0..3 {
        if dir[a].abs() < 1e-8 {
            if origin[a] < min[a] || origin[a] > max[a] {
                return false;
            }
            continue;
        }
        let (mut near, mut far) = ((min[a] - origin[a]) / dir[a], (max[a] - origin[a]) / dir[a]);
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        t0 = t0.max(near);
        t1 = t1.min(far);
        if t0 > t1 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rayo_contra_caja() {
        let (min, max) = (Vec3::new(-40.0, -40.0, 0.0), Vec3::new(40.0, 40.0, 80.0));
        // From above and in front, looking down at the center.
        assert!(ray_hits_box(Vec3::new(0.0, -500.0, 1000.0), Vec3::new(0.0, 500.0, -960.0).normalize(), min, max));
        // Passes beside it.
        assert!(!ray_hits_box(Vec3::new(200.0, -500.0, 1000.0), Vec3::new(0.0, 500.0, -960.0).normalize(), min, max));
        // The box is behind.
        assert!(!ray_hits_box(Vec3::new(0.0, 0.0, 1000.0), Vec3::Z, min, max));
    }

    #[test]
    fn reglas() {
        assert_eq!(rule(1, 133).map(|r| r.act), Some(Act::Pose));
        assert_eq!(rule(1, 146).map(|r| r.act), Some(Act::Sit));
        assert_eq!(rule(4, 38).map(|r| r.act), Some(Act::Healing));
        assert!(rule(1, 59).is_none());
    }
}
