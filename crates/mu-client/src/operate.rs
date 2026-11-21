//! Objetos del mapa donde el personaje se sienta, se apoya o se cura (`OPERATE`, `CreateOperate`
//! en ZzzObject.cpp ≈4551 y los `CreateObject` de cada mapa), y lo que hay bajo el ratón para el
//! cursor (`SelectOperate`, `DontMove`).
//!
//! - Cada mapa marca unos tipos de objeto (bancos, sillas, troncos, cajas de pose invisibles...).
//! - `SelectOperate` (ZzzInterface.cpp ≈7815): el rayo del ratón contra la caja de cada uno (la
//!   posición del objeto más `BoundingBoxMin..Max`, sin girar ni escalar; por defecto
//!   (-40,-40,0)..(40,40,80)). El primero que toca se ilumina (`Light` = 1.5) y los demás quedan
//!   con 0.2 (se suma a la luz del terreno, `VectorAdd(Light, o->Light, BodyLight)`).
//! - Al hacer clic se va hasta su casilla y, al llegar, `Action` con `MOVEMENT_OPERATE`
//!   (ZzzInterface.cpp ≈3587) pone la animación de sentarse, de pose o de curarse.

use bevy::prelude::*;

use crate::model_light::ModelMaterial;
use crate::terrain::MU_SCALE;

pub struct OperatePlugin;

impl Plugin for OperatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Operates>().init_resource::<Pointer>().add_systems(
            Update,
            select_operate
                .before(crate::character::HeroInput)
                .run_if(in_state(crate::scene::GameScene::Game)),
        );
    }
}

/// Lo que hace el personaje al llegar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    /// `PLAYER_SIT1`.
    Sit,
    /// `PLAYER_POSE1`: apoyado.
    Pose,
    /// `PLAYER_HEALING1` (la fuente de Noria).
    Healing,
    /// Se puede señalar pero no hace nada (el trono de Valley of Loren, solo para el maestro del
    /// gremio que tiene el castillo).
    Nothing,
}

/// Regla de un tipo de objeto en un mapa.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub act: Act,
    /// `Hero->Object.Angle[2] = TargetAngle`: mira hacia donde mira el objeto.
    pub face: bool,
    /// Cursor `CursorLeanAgainst` (en vez de `CursorSitDown`), ver `RenderCursor`.
    pub lean_cursor: bool,
    /// `BoundingBoxMax` (el mínimo es siempre (-40, -40, 0)).
    pub bbox_max: Vec3,
}

const BOX: Vec3 = Vec3::new(40.0, 40.0, 80.0);
/// Cajas de pose: `Vector(40, 40, 160, BoundingBoxMax)`.
const TALL: Vec3 = Vec3::new(40.0, 40.0, 160.0);

/// ¿Es `kind` un objeto de sentarse/apoyarse en el mapa `world` (número de carpeta, `WorldActive` + 1)?
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
        // Noria: la fuente cura (y lleva el cursor de apoyarse).
        4 => match k {
            8 => r(Act::Sit, false, false, BOX),
            38 => r(Act::Healing, true, true, BOX),
            _ => None,
        },
        // Atlans.
        8 if k == 39 => r(Act::Pose, true, false, BOX),
        // Tarkan.
        9 if k == 78 => r(Act::Sit, false, false, BOX),
        // Valley of Loren (castillo): 77 es el trono del maestro del gremio.
        31 => match k {
            77 => r(Act::Nothing, true, false, BOX),
            84 => r(Act::Sit, true, false, BOX),
            _ => None,
        },
        // Kanturu (ruinas, `M38Kanturu2nd`).
        40 if k == 3 => r(Act::Sit, false, false, BOX),
        // Elbeland (`GMNewTown`).
        52 if k == 103 => r(Act::Sit, true, false, BOX),
        // Loren Market (`GMUnitedMarketPlace`): caja de pose de 100×100×160.
        80 if k == 67 => r(Act::Pose, true, false, Vec3::new(100.0, 100.0, 160.0)),
        _ => None,
    }
}

/// Un objeto de sentarse del mapa cargado.
pub struct Operate {
    pub rule: Rule,
    /// Posición en MU.
    pub position: Vec3,
    /// `Angle[2]` en grados.
    pub angle: f32,
    /// Materiales propios de sus mallas iluminadas (para resaltarlo) y la luz del terreno que tienen.
    pub materials: Vec<Handle<ModelMaterial>>,
    pub light: Vec3,
    /// `o->Light` actual (0 hasta que el ratón pasa por el juego; luego 0.2 o 1.5).
    pub glow: f32,
}

impl Operate {
    /// Casilla del objeto (`Position / TERRAIN_SCALE`).
    pub fn tile(&self) -> (i32, i32) {
        ((self.position.x / MU_SCALE) as i32, (self.position.y / MU_SCALE) as i32)
    }
}

#[derive(Resource, Default)]
pub struct Operates(pub Vec<Operate>);

/// Lo que hay bajo el ratón, para el cursor y el clic.
#[derive(Resource, Default)]
pub struct Pointer {
    /// `SelectedOperate`.
    pub operate: Option<usize>,
    /// `DontMove`: el último clic en el suelo fue en una casilla a la que no se puede ir.
    pub dont_move: bool,
}

impl Pointer {
    /// Cursor del objeto señalado: `Some(true)` apoyarse, `Some(false)` sentarse.
    pub fn operate_cursor(&self, operates: &Operates) -> Option<bool> {
        self.operate.and_then(|i| operates.0.get(i)).map(|o| o.rule.lean_cursor)
    }
}

/// Al cambiar de mapa.
pub fn reset(mut operates: ResMut<Operates>, mut pointer: ResMut<Pointer>) {
    operates.0.clear();
    *pointer = Pointer::default();
}

/// `o->Light` del objeto no señalado y del señalado.
const GLOW_IDLE: f32 = 0.2;
const GLOW_SELECTED: f32 = 1.5;

/// Rayo del ratón en coordenadas de MU (origen, dirección), si el ratón está en la ventana.
pub fn mouse_ray_mu(window: &Window, camera: &Camera, cam_tf: &GlobalTransform) -> Option<(Vec3, Vec3)> {
    let ray = camera.viewport_to_world(cam_tf, window.cursor_position()?).ok()?;
    let to_mu = |v: Vec3| Vec3::new(v.x, -v.z, v.y);
    Some((to_mu(ray.origin) * MU_SCALE, to_mu(*ray.direction)))
}

/// `SelectObjects` → `SelectOperate`: solo con el ratón en la zona de juego (por encima de la barra,
/// `CheckMouseIn(0, 0, ancho, 429)`) y fuera de las ventanas.
fn select_operate(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    mut operates: ResMut<Operates>,
    mut pointer: ResMut<Pointer>,
    mut materials: ResMut<Assets<ModelMaterial>>,
) {
    pointer.operate = None;
    let (Ok(window), Ok((camera, cam_tf))) = (windows.single(), cameras.single()) else { return };
    let in_area = crate::mainframe::cursor_virtual(window).is_some_and(|c| c.y < 429.0);
    if over_ui.0 || !in_area {
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

/// `CollisionDetectLineToOBB` con una caja alineada a los ejes (método de las franjas).
fn ray_hits_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> bool {
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
        // Desde arriba y por delante, mirando hacia abajo al centro.
        assert!(ray_hits_box(Vec3::new(0.0, -500.0, 1000.0), Vec3::new(0.0, 500.0, -960.0).normalize(), min, max));
        // Pasa por al lado.
        assert!(!ray_hits_box(Vec3::new(200.0, -500.0, 1000.0), Vec3::new(0.0, 500.0, -960.0).normalize(), min, max));
        // La caja está detrás.
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
