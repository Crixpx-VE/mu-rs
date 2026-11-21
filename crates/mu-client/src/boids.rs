//! Bichos que vuelan alrededor del personaje (`MoveBoids` / `RenderBoids` de GOBoid.cpp,
//! `MoveBoid` de ZzzAI.cpp) y lo común con los del suelo y el agua (`fish.rs`).
//!
//! Por mapa (`gMapManager.WorldActive`, = `Args.world` − 1), GOBoid.cpp:1218-1382:
//! - Lorencia: pájaros (`Object1/Bird01`). Dungeon y Lost Tower: murciélagos (`Object2/Bat01`).
//!   Noria: mariposas (`Object1/Butterfly01`) con su brillo. Blood Castle: cuervos
//!   (`Object12/Crow01`) con los ojos rojos. 5 a la vez.
//! - Icarus (`CreateDragon`, :818): 3 dragones (`Monster32`) 600 por debajo del personaje y 10
//!   bichos invisibles (`MODEL_SPEARSKILL`) que arrastran una estela `MODEL_SPEARSKILL` 1.
//! - Atlans (`CreateAtlanseFish`, :880): hasta 40 peces (`Object8/Fish02-03`) en banco, solo en la
//!   mitad sur del mapa y si el personaje no está en una casilla con atributos.
//! - Kalima (`CreateBigMon`, GMHellas.cpp:633): 2 huecos; de vez en cuando un monstruo enorme
//!   (`Monster34`, negro) cruza muy por debajo de las nubes.
//! - Elbeland: un tornado (`Object52/typhoonall`, :1345) fuera de la zona segura y el águila
//!   (`Object52/sos3bi01`) que suelta el objeto 62 del mapa (GMNewTown.cpp:175).
//! - Durante la invasión (ver `invasion.rs`) los huecos se llenan de dragones que cruzan hacia −Y.
//!
//! Vuelo en grupo (`MoveBoid`): cada uno gira hasta `Gravity` grados por tick hacia la media de las
//! direcciones de los vecinos (a menos de 400) y se aparta de los que tiene a menos de 80.
//!
//! Como las partículas: la lógica corre a 25 ticks por segundo y el dibujo interpola.

use std::f32::consts::FRAC_PI_2;
use std::path::Path;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::math::EulerRot;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::terrain::{SIZE, Terrain, attr};
use mu_data::{fs, texture};

use crate::Args;
use crate::character::Player;
use crate::effects::FireRequests;
use crate::fx::FxWorld;
use crate::fx::core::{self as fxc, Blend, Rng, Spawn, SpriteDraw, Target};
use crate::invasion::Invasion;
use crate::model_light::{ModelLight, ModelMaterial, model_material};
use crate::terrain::{LoadedTerrain, MU_SCALE, to_bevy_image};

pub struct BoidsPlugin;

impl Plugin for BoidsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BoidSprites>()
            .add_systems(Update, setup_boids.run_if(crate::scene::load_step(crate::scene::Step::Boids)))
            // Antes del tick de efectos: las estelas que siguen a un bicho ven ya su posición nueva.
            .add_systems(FixedUpdate, step_boids.before(crate::fx::sim::tick))
            .add_systems(Update, draw_boids)
            .add_systems(PostUpdate, push_sprites.after(crate::fx::draw::collect).before(crate::fx::draw::render_quads));
    }
}

/// `MAX_BOIDS` (_define.h:126).
const MAX_BOIDS: usize = 40;
const NEIGHBOR_RANGE: f32 = 400.0;
const SEPARATION_RANGE: f32 = 80.0;
/// Acción de los dragones (`MONSTER01_DIE + 1`).
const DRAGON_ACTION: usize = 7;
/// `MONSTER01_WALK`.
const MONSTER_WALK: usize = 2;
/// Hueso de la cabeza del dragón, donde el original pone el fuego.
const DRAGON_HEAD_BONE: usize = 11;
/// Tipo del objeto de Elbeland que suelta el águila (`GMNewTown::MoveObject`, case 62).
const EAGLE_PERCH: i16 = 62;

/// Acciones del pájaro.
const BIRD_FLY: usize = 0;
const BIRD_GROUND: usize = 1;

// --- Lo común con `fish.rs` -------------------------------------------------------------------------

/// `AI` de los bichos (`BOID_FLY`...).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Ai {
    Fly,
    Down,
    Ground,
    Up,
}

/// Qué es cada hueco (el `o->Type` del original).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Bird,
    Bat,
    Butterfly,
    Crow,
    Tornado,
    Eagle,
    /// `MODEL_MONSTER01+31` de Icarus.
    HeavenDragon,
    /// `MODEL_SPEARSKILL` de Icarus: no se dibuja, solo lleva su estela.
    HeavenBug,
    /// `MODEL_FISH01+1..2` en banco (Atlans).
    AtlansFish,
    /// `MODEL_MONSTER01+33` de Kalima.
    BigMon,
    /// `MODEL_MONSTER01+31` de la invasión (`EnableEvent`).
    InvasionDragon,
    // `Fishs[]` (fish.rs).
    /// `MODEL_FISH01` de Lorencia.
    Fish,
    Rat,
    /// `MODEL_BUG01` de Arena.
    Bug,
    /// `MODEL_BUG01+1` de Tarkan y Aida (luciérnaga con estela).
    Firefly,
    Scorpion,
    /// `MODEL_FISH01+3..6` de Atlans.
    SeaFish,
    /// `MODEL_FISH01+7..8` de Atlans: malla 0 sumando y sin sombra.
    GlowFish,
    /// `Type = -1` de Kalima: solo la estela `BITMAP_FLARE+1`.
    Flare,
}

/// Un modelo cargado y sus texturas.
pub(crate) struct Model {
    pub bmd: Arc<Bmd>,
    pub textures: Vec<Option<Handle<Image>>>,
    /// `HiddenMesh`: malla que no se dibuja (−1 = ninguna).
    pub hidden_mesh: i32,
}

pub(crate) fn load_model(data: &Path, dir: &str, file: &str, images: &mut Assets<Image>) -> Option<Model> {
    let dir = data.join(dir);
    let bmd = match fs::read(&dir, file).and_then(|b| bmd::read(&b)) {
        Ok(m) => Arc::new(m),
        Err(e) => {
            warn!("sin {file}: {e}");
            return None;
        }
    };
    let textures = bmd
        .meshes
        .iter()
        .map(|m| texture::load(&dir, &bmd::texture_file_name(&m.texture)).ok().map(|t| images.add(to_bevy_image(&t))))
        .collect();
    Some(Model { bmd, textures, hidden_mesh: -1 })
}

struct Part {
    entity: Entity,
    mesh_index: usize,
    texture: i16,
    mesh: Handle<Mesh>,
    material: Handle<ModelMaterial>,
}

/// Un modelo dibujable para un hueco: entidad con una malla por cada malla del `.bmd`.
pub(crate) struct Body {
    entity: Entity,
    parts: Vec<Part>,
    /// Último estado del material (para no rehacerlo si no cambia).
    shown: Option<[i32; 6]>,
}

fn spawn_body(
    commands: &mut Commands,
    name: String,
    model: &Model,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ModelMaterial>,
) -> Body {
    let mut parts = vec![];
    let entity = commands
        .spawn((Name::new(name), Transform::default(), Visibility::Hidden, crate::scene::WorldEntity))
        .with_children(|p| {
            for (mi, tex) in model.textures.iter().enumerate() {
                if mi as i32 == model.hidden_mesh {
                    continue;
                }
                let posed = model.bmd.pose(mi, 0, 0);
                if posed.positions.is_empty() {
                    continue;
                }
                let mesh = meshes.add(
                    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs),
                );
                let base = StandardMaterial {
                    base_color_texture: tex.clone(),
                    unlit: true,
                    cull_mode: None,
                    alpha_mode: AlphaMode::Mask(crate::objects::ALPHA_REF),
                    ..default()
                };
                let material = materials.add(model_material(base, ModelLight::unlit()));
                let entity = p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()))).id();
                parts.push(Part { entity, mesh_index: mi, texture: model.bmd.meshes[mi].texture_index, mesh, material });
            }
        })
        .id();
    Body { entity, parts, shown: None }
}

/// Un hueco de `Boids[]` / `Fishs[]` (el `OBJECT` del original, con lo que usan estos bichos).
pub(crate) struct Obj {
    pub live: bool,
    pub kind: Kind,
    /// Índice en la tabla de modelos de la lista (`None` = no se dibuja).
    pub model: Option<usize>,
    pub pos: Vec3,
    pub prev_pos: Vec3,
    /// Grados (x, y, z) como `o->Angle`.
    pub angle: Vec3,
    pub prev_angle: Vec3,
    pub head_angle: Vec3,
    /// `Direction`: (x, y) punto al que va, z velocidad vertical.
    pub dir: Vec3,
    pub velocity: f32,
    pub gravity: f32,
    pub timer: f32,
    pub life: i32,
    pub sub_type: i32,
    pub ai: Ai,
    pub action: usize,
    pub frame: f32,
    pub prev_frame: f32,
    pub scale: f32,
    pub alpha: f32,
    pub prev_alpha: f32,
    pub alpha_target: f32,
    pub alpha_enable: bool,
    pub light: Vec3,
    pub light_enable: bool,
    pub blend_mesh: i32,
    pub blend_mesh_light: f32,
    /// Se dibuja su sombra (`RenderBodyShadow`).
    pub shadow_on: bool,
    /// Dragón dorado de la invasión (`SubType = 1`).
    pub golden: bool,
    /// Marca invisible del motor de efectos que hace de `o` para sus estelas (`Target::Effect`).
    pub anchor: Option<u32>,
    bodies: Vec<Option<Body>>,
    shadow: crate::shadow::Shadow,
}

impl Obj {
    pub(crate) fn new(shadow: crate::shadow::Shadow) -> Self {
        Self {
            live: false,
            kind: Kind::Bird,
            model: None,
            pos: Vec3::ZERO,
            prev_pos: Vec3::ZERO,
            angle: Vec3::ZERO,
            prev_angle: Vec3::ZERO,
            head_angle: Vec3::ZERO,
            dir: Vec3::ZERO,
            velocity: 1.0,
            gravity: 13.0,
            timer: 0.0,
            life: 0,
            sub_type: 0,
            ai: Ai::Fly,
            action: 0,
            frame: 0.0,
            prev_frame: 0.0,
            scale: 0.8,
            alpha: 0.0,
            prev_alpha: 0.0,
            alpha_target: 1.0,
            alpha_enable: false,
            light: Vec3::ONE,
            light_enable: true,
            blend_mesh: -1,
            blend_mesh_light: 1.0,
            shadow_on: true,
            golden: false,
            anchor: None,
            bodies: vec![],
            shadow,
        }
    }

    /// `o->Initialize()` (lo que usan estos bichos): todo a cero salvo los modelos y la sombra.
    pub(crate) fn initialize(&mut self) {
        let bodies = std::mem::take(&mut self.bodies);
        let shadow = self.shadow.clone();
        let anchor = self.anchor.take();
        *self = Obj::new(shadow);
        self.bodies = bodies;
        self.anchor = anchor;
        self.velocity = 0.0;
        self.gravity = 0.0;
        self.scale = 0.0;
        self.alpha_target = 0.0;
        self.light = Vec3::ZERO;
    }

    /// Guarda el estado del tick anterior (para interpolar).
    pub(crate) fn remember(&mut self) {
        self.prev_pos = self.pos;
        self.prev_angle = self.angle;
        self.prev_frame = self.frame;
        self.prev_alpha = self.alpha;
    }

    /// Empieza a vivir aquí: sin interpolar desde donde estaba antes.
    pub(crate) fn place(&mut self, pos: Vec3) {
        self.pos = pos;
        self.prev_pos = pos;
        self.prev_angle = self.angle;
        self.prev_frame = self.frame;
        self.prev_alpha = self.alpha;
    }

    /// `Alpha(o)` (ZzzAI.cpp:169).
    pub(crate) fn alpha_step(&mut self) {
        if self.alpha_enable {
            if self.alpha_target > self.alpha {
                self.alpha = (self.alpha + 0.05).min(1.0);
            } else if self.alpha_target < self.alpha {
                self.alpha = (self.alpha - 0.05).max(0.0);
            }
        } else {
            self.alpha += (self.alpha_target - self.alpha) * 0.1;
        }
        if self.blend_mesh_light > self.alpha {
            self.blend_mesh_light = self.alpha;
        }
    }

    /// Lleva su marca a donde está ahora, o la quita si ha muerto.
    pub(crate) fn sync_anchor(&mut self, fx: &mut FxWorld) {
        let Some(id) = self.anchor else { return };
        if self.live {
            if let Some(e) = fx.effects.iter_mut().find(|e| e.id == id) {
                e.position = self.pos;
                e.angle = self.angle;
                e.life_time = ANCHOR_LIFE;
            }
        } else {
            fx.effects.retain(|e| e.id != id);
            self.anchor = None;
        }
    }

    pub(crate) fn kill(&mut self, fx: &mut FxWorld) {
        self.live = false;
        self.sync_anchor(fx);
    }
}

/// Tipo (sin código registrado) de la marca invisible que sigue a un bicho.
const BOID_ANCHOR: i32 = -2_000_101;
const ANCHOR_LIFE: f32 = 1.0e9;

/// Crea la marca que hará de `o` para lo que el bicho cree con `CreateJoint(..., o)`.
pub(crate) fn spawn_anchor(o: &mut Obj, fx: &mut FxWorld) -> Target {
    if let Some(id) = o.anchor.take() {
        fx.effects.retain(|e| e.id != id);
    }
    fx.next_effect_id += 1;
    let id = fx.next_effect_id;
    let mut e = fxc::Effect::new(id, BOID_ANCHOR, o.pos, o.angle, Vec3::ONE, 0, None, -1, 0, 0, 1.0);
    e.visible = false;
    e.life_time = ANCHOR_LIFE;
    fx.effects.push(e);
    o.anchor = Some(id);
    Target::Effect(id)
}

/// `CreateJoint(Type, Position, Position, Angle, SubType, o, Scale)`.
pub(crate) fn joint_on(o: &mut Obj, fx: &mut FxWorld, ty: i32, sub_type: i32, scale: f32) {
    let target = spawn_anchor(o, fx);
    let j = fxc::Joint::new(ty, o.pos, o.pos, o.angle, sub_type, Some(target), scale);
    crate::fx::request(fx, Spawn::Joint(j));
}

/// Lo que miran del personaje (`Hero`).
pub(crate) struct Hero {
    /// Posición en MU (la altura, la del terreno).
    pub pos: Vec3,
    /// `TerrainWall` de su casilla.
    pub wall: u16,
    /// `Hero->SafeZone`.
    pub safe: bool,
    /// Parado (`CurrentAction < PLAYER_WALK_MALE`).
    pub still: bool,
}

impl Hero {
    pub(crate) fn of(p: &Player, t: &Terrain) -> Self {
        let pos = Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, t.height_at(p.pos.x, p.pos.y));
        let wall = wall_at(t, pos.x, pos.y);
        Self { pos, wall, safe: wall & attr::SAFE_ZONE != 0, still: p.path.is_empty() }
    }

    pub(crate) fn range(&self, p: Vec3) -> f32 {
        (p.truncate() - self.pos.truncate()).length()
    }
}

/// `TERRAIN_INDEX_REPEAT(x / TERRAIN_SCALE, y / TERRAIN_SCALE)` con x, y en MU.
pub(crate) fn tile_index(x: f32, y: f32) -> usize {
    let tx = (x / MU_SCALE) as i32 & 255;
    let ty = (y / MU_SCALE) as i32 & 255;
    (ty * SIZE as i32 + tx) as usize
}

/// `TerrainWall[TERRAIN_INDEX_REPEAT(...)]`.
pub(crate) fn wall_at(t: &Terrain, x: f32, y: f32) -> u16 {
    t.attributes.get(tile_index(x, y)).copied().unwrap_or(0)
}

/// `TerrainWall == 0 || TerrainWall == TW_CHARACTER`.
pub(crate) fn open_wall(w: u16) -> bool {
    w == 0 || w == attr::CHARACTER
}

/// `RequestTerrainHeight(x, y)` con x, y en MU.
pub(crate) fn ground(t: &Terrain, p: Vec3) -> f32 {
    t.height_at(p.x / MU_SCALE, p.y / MU_SCALE)
}

/// `RequestTerrainLight(x, y)` (ZzzLodTerrain.cpp:933): luz del terreno interpolada.
fn terrain_light(t: &Terrain, x: f32, y: f32) -> Vec3 {
    let (xf, yf) = (x / MU_SCALE, y / MU_SCALE);
    let (xi, yi) = (xf as i32, yf as i32);
    if xi < 0 || yi < 0 || xi > SIZE as i32 - 2 || yi > SIZE as i32 - 2 {
        return Vec3::ZERO;
    }
    let (xi, yi) = (xi as usize, yi as usize);
    let l = |x: usize, y: usize| Vec3::from_array(t.light[y * SIZE + x]);
    let (xd, yd) = (xf - xi as f32, yf - yi as f32);
    let left = l(xi, yi) + (l(xi, yi + 1) - l(xi, yi)) * yd;
    let right = l(xi + 1, yi) + (l(xi + 1, yi + 1) - l(xi + 1, yi)) * yd;
    left + (right - left) * xd
}

/// `CalcAngle` (ZzzAI.cpp): rumbo entero en grados con 0 = +X y 90 = +Y (el de `AngleMatrix`).
pub(crate) fn calc_angle(px: f32, py: f32, tx: f32, ty: f32) -> i32 {
    let grad = if tx - px == 0.0 { 0.0 } else { (py - ty) / (tx - px) };
    let mut theta = (grad.atan() * (180.0 / std::f32::consts::PI)) as i32;
    if tx < px {
        theta += 180;
    }
    if theta < 0 {
        theta += 360;
    }
    (360 - theta) % 360
}

/// `TurnAngle(iTheta, iHeading, maxTURN)` (ZzzAI.cpp), en enteros como el original.
pub(crate) fn turn_angle_int(theta: i32, heading: i32, max_turn: i32) -> i32 {
    let mut change = 0;
    let delta = (theta - heading).abs();
    if theta > heading {
        change = if delta < ((heading + 360) - theta).abs() { -max_turn.min(delta) } else { max_turn.min(delta) };
    }
    if theta < heading {
        change = if delta < ((theta + 360) - heading).abs() { max_turn.min(delta) } else { -max_turn.min(delta) };
    }
    (theta + change + 360) % 360
}

/// `MoveBoid(o, i, Boids, MAX)` (ZzzAI.cpp:190).
pub(crate) fn move_boid(list: &mut [Obj], i: usize) {
    let me = list[i].pos;
    let (mut tx, mut ty, mut n) = (0.0f32, 0.0f32, 0);
    for (j, t) in list.iter().enumerate() {
        if !t.live || j == i {
            continue;
        }
        let d = (me.truncate() - t.pos.truncate()).length();
        if d >= NEIGHBOR_RANGE {
            continue;
        }
        let (mut xd, mut yd) = (t.dir.x - t.pos.x, t.dir.y - t.pos.y);
        if d < SEPARATION_RANGE {
            xd -= t.dir.x - me.x;
            yd -= t.dir.y - me.y;
        } else {
            xd += t.dir.x - me.x;
            yd += t.dir.y - me.y;
        }
        let p = (xd * xd + yd * yd).sqrt();
        // (con p = 0 el original divide por cero y gira hacia cualquier lado)
        if p > 0.0 {
            tx += xd / p;
            ty += yd / p;
            n += 1;
        }
    }
    if n > 0 {
        let goal = (me.x + tx / n as f32, me.y + ty / n as f32);
        let o = &mut list[i];
        o.angle.z = turn_angle_int(o.angle.z as i32, calc_angle(me.x, me.y, goal.0, goal.1), o.gravity as i32) as f32;
    }
}

/// Fotogramas de una acción (1 si no existe).
pub(crate) fn frames(model: &Bmd, action: usize) -> f32 {
    model.actions.get(action).map_or(1, |a| a.frames.max(1)) as f32
}

/// `PlayAnimation`: avanza `speed` fotogramas y da la vuelta al acabar.
pub(crate) fn play_animation(o: &mut Obj, models: &[Option<Model>], speed: f32) {
    let Some(m) = o.model.and_then(|m| models.get(m)).and_then(Option::as_ref) else { return };
    let n = frames(&m.bmd, o.action);
    if n > 1.0 {
        o.frame = (o.frame + speed).rem_euclid(n);
    }
}

/// Un sprite de un fotograma (`CreateSprite` desde `RenderBoids`).
#[derive(Clone, Copy)]
pub(crate) struct BoidSprite {
    texture: i32,
    position: Vec3,
    scale: f32,
    light: Vec3,
}

/// Sprites de este fotograma, ya en su posición interpolada.
#[derive(Resource, Default)]
pub(crate) struct BoidSprites(Vec<BoidSprite>);

/// Coloca, gira, anima y da luz a cada hueco interpolando entre los dos últimos ticks
/// (`RenderObject` + `RenderBodyShadow` de `RenderBoids` / `RenderFishs`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_objs(
    list: &mut [Obj],
    models: &[Option<Model>],
    a: f32,
    terrain: Option<&Terrain>,
    commands: &mut Commands,
    q: &mut Query<(&mut Transform, &mut Visibility)>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ModelMaterial>,
    sprites: &mut Vec<BoidSprite>,
    rng: &mut Rng,
    label: &str,
) {
    let to_bevy = Quat::from_rotation_x(-FRAC_PI_2);
    for (slot, o) in list.iter_mut().enumerate() {
        let alpha = o.prev_alpha + (o.alpha - o.prev_alpha) * a;
        // `Calc_RenderObject` no dibuja con alfa < 0.01.
        let current = o.model.filter(|&m| o.live && alpha >= 0.01 && models.get(m).is_some_and(Option::is_some));
        for (mi, body) in o.bodies.iter().enumerate() {
            if let Some(body) = body
                && Some(mi) != current
                && let Ok((_, mut vis)) = q.get_mut(body.entity)
            {
                vis.set_if_neq(Visibility::Hidden);
            }
        }
        let Some(mi) = current else {
            let v = q.get_mut(o.shadow.1).ok().map(|(_, v)| v);
            crate::shadow::Builder::new(None, Vec3::ZERO, Quat::IDENTITY, 1.0).finish(&o.shadow, meshes, v);
            continue;
        };
        let model = models[mi].as_ref().expect("filtrado arriba");
        if o.bodies.len() <= mi {
            o.bodies.resize_with(mi + 1, || None);
        }
        let body = o.bodies[mi].get_or_insert_with(|| {
            spawn_body(commands, format!("{label} {slot}"), model, meshes, materials)
        });

        let pos = o.prev_pos.lerp(o.pos, a);
        let d = |x: f32, y: f32| (y - x + 540.0).rem_euclid(360.0) - 180.0;
        let angle = Vec3::new(
            o.prev_angle.x + d(o.prev_angle.x, o.angle.x) * a,
            o.prev_angle.y + d(o.prev_angle.y, o.angle.y) * a,
            o.prev_angle.z + d(o.prev_angle.z, o.angle.z) * a,
        );
        // El original los dibuja girados un cuarto de vuelta (`o->Angle[2] += 90`).
        let rot = Quat::from_euler(
            EulerRot::ZYX,
            (angle.z + 90.0).to_radians(),
            angle.y.to_radians(),
            angle.x.to_radians(),
        );
        if let Ok((mut tf, mut vis)) = q.get_mut(body.entity) {
            vis.set_if_neq(Visibility::Inherited);
            *tf = Transform {
                translation: Vec3::new(pos.x, pos.z, -pos.y) / MU_SCALE,
                rotation: to_bevy * rot,
                scale: Vec3::splat(o.scale / MU_SCALE),
            };
        }
        let n = frames(&model.bmd, o.action);
        let mut step = o.frame - o.prev_frame;
        if step < 0.0 {
            step += n;
        }
        // Un salto (fotograma puesto a mano) no se interpola.
        let frame = if step <= 2.0 { (o.prev_frame + step * a) % n } else { o.frame };
        let action = if o.action < model.bmd.actions.len() { o.action } else { 0 };
        let bones = model.bmd.bone_matrices_at(action, frame);

        let mut sh = crate::shadow::Builder::new(terrain.filter(|_| o.shadow_on), pos, rot, o.scale);
        for part in &body.parts {
            let posed = model.bmd.pose_with(part.mesh_index, &bones);
            // `RenderBodyShadow` se salta la malla que suma (`m->Texture == BlendMesh`).
            if o.shadow_on && part.texture as i32 != o.blend_mesh {
                sh.add(&posed.positions);
            }
            if let Some(mut m) = meshes.get_mut(&part.mesh) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
            }
        }
        let v = q.get_mut(o.shadow.1).ok().map(|(_, v)| v);
        sh.finish(&o.shadow, meshes, v);

        // `BodyLight` (ZzzObject.cpp:206): luz del terreno + `o->Light` (o la décima parte del
        // terreno si `LightEnable` es falso). El monstruo de Kalima se dibuja negro
        // (`RenderHellasObjectMesh`, GMHellas.cpp:515).
        let terrain_l = terrain.map_or(Vec3::ONE, |t| terrain_light(t, pos.x, pos.y));
        let body_light = match o.kind {
            Kind::BigMon => Vec3::ZERO,
            Kind::InvasionDragon if o.golden => Vec3::new(1.6, 1.2, 0.45),
            _ if o.light_enable => terrain_l + o.light,
            _ => terrain_l * 0.1 + o.light,
        };
        let blend_light = o.blend_mesh_light;
        let q100 = |v: f32| (v * 100.0).round() as i32;
        let key = [q100(alpha), q100(body_light.x), q100(body_light.y), q100(body_light.z), q100(blend_light), o.blend_mesh];
        if body.shown != Some(key) {
            body.shown = Some(key);
            for part in &body.parts {
                let additive = o.blend_mesh <= -2 || (o.blend_mesh >= 0 && part.texture as i32 == o.blend_mesh);
                // Con alfa < 1 el original mezcla con prueba de alfa (> 0.25): por debajo no se ve.
                let show = additive || alpha >= 0.99 || alpha > crate::objects::ALPHA_REF;
                if let Ok((_, mut vis)) = q.get_mut(part.entity) {
                    vis.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
                }
                let Some(mut m) = materials.get_mut(&part.material) else { continue };
                if additive {
                    let l = (body_light * blend_light).clamp(Vec3::ZERO, Vec3::ONE).powf(2.2);
                    m.base.base_color = Color::linear_rgb(l.x, l.y, l.z);
                    m.base.alpha_mode = AlphaMode::Add;
                    m.extension = ModelLight::unlit();
                    continue;
                }
                if o.light_enable || o.kind == Kind::BigMon {
                    m.base.base_color = Color::WHITE;
                    m.extension = ModelLight::lit(body_light.to_array());
                } else {
                    let l = body_light.clamp(Vec3::ZERO, Vec3::ONE).powf(2.2);
                    m.base.base_color = Color::linear_rgb(l.x, l.y, l.z);
                    m.extension = ModelLight::unlit();
                }
                if alpha >= 0.99 {
                    m.base.base_color.set_alpha(1.0);
                    m.base.alpha_mode = AlphaMode::Mask(crate::objects::ALPHA_REF);
                } else {
                    m.base.base_color.set_alpha(alpha);
                    m.base.alpha_mode = AlphaMode::Blend;
                }
            }
        }

        // Lo que `RenderBoids` crea al dibujar (GOBoid.cpp:1535-1582).
        let at = |bone: usize, off: [f32; 3]| match bones.get(bone) {
            Some(m) => pos + rot * (Vec3::from_array(bmd::transform_point(m, off)) * o.scale),
            None => pos,
        };
        match o.kind {
            Kind::Butterfly => {
                let lum = (rng.rand() % 32 + 64) as f32 * 0.01;
                sprites.push(BoidSprite {
                    texture: fxc::BITMAP_LIGHT,
                    position: pos,
                    scale: 1.0,
                    light: Vec3::new(lum * 0.2, lum * 0.4, lum * 0.4),
                });
            }
            Kind::Crow => {
                let lum = (rng.rand() % 32 + 128) as f32 * 0.01;
                let light = Vec3::new(lum, lum * 0.2, 0.0);
                for x in [-5.0, 5.0] {
                    sprites.push(BoidSprite { texture: fxc::BITMAP_LIGHT, position: at(1, [x, 0.0, 0.0]), scale: 0.1, light });
                }
            }
            Kind::InvasionDragon => {
                sprites.push(BoidSprite {
                    texture: fxc::BITMAP_LIGHTNING + 1,
                    position: at(DRAGON_HEAD_BONE, [0.0, -50.0, 0.0]),
                    scale: 1.0,
                    light: Vec3::new(1.0, 0.0, 0.0),
                });
            }
            _ => {}
        }
    }
}

/// Pasa los sprites de los bichos a la lista de dibujo de los efectos (`DescribeSprite`, subtipo 0).
fn push_sprites(
    mut list: ResMut<BoidSprites>,
    mut frame: ResMut<crate::fx::draw::FrameDraw>,
    mut textures: ResMut<crate::fx::textures::FxTextures>,
    mut images: ResMut<Assets<Image>>,
) {
    for s in std::mem::take(&mut list.0) {
        let Some(t) = textures.info(s.texture, &mut images) else { continue };
        frame.0.sprites.push(SpriteDraw {
            texture: s.texture,
            position: s.position,
            width: t.width * s.scale,
            height: t.height * s.scale,
            color: [s.light.x, s.light.y, s.light.z, if t.components == 3 { 1.0 } else { s.light.x }],
            rotation: 0.0,
            uv: [0.0, 0.0, 1.0, 1.0],
            blend: Blend::Glow,
        });
    }
}

// --- Boids[] ---------------------------------------------------------------------------------------

/// Modelos de la lista de `Boids[]`.
const M_MAIN: usize = 0;
const M_DRAGON: usize = 1;
const M_FISH2: usize = 2;
const M_BIGMON: usize = 4;
const M_EAGLE: usize = 5;

fn in_blood_castle(wa: i32) -> bool {
    (11..=17).contains(&wa) || wa == 52
}

/// `InHellas` (Kalima 1-7).
pub(crate) fn in_hellas(wa: i32) -> bool {
    (24..=29).contains(&wa) || wa == 36
}

fn in_atlans(wa: i32) -> bool {
    wa == 7 || wa == 67
}

/// Huecos que usa cada mapa (el `bOut` de `MoveBoids`, GOBoid.cpp:1222-1267).
fn boid_slots(wa: i32) -> usize {
    match wa {
        7 | 67 => MAX_BOIDS,
        10 => 13,
        51 => 2,
        _ if in_hellas(wa) => 2,
        _ => 5,
    }
}

#[derive(Resource)]
struct Boids {
    /// `WorldActive`.
    wa: i32,
    models: Vec<Option<Model>>,
    list: Vec<Obj>,
    rng: Rng,
    event: Invasion,
    /// `MoveSceneFrame`.
    tick: u32,
    /// Objetos 62 de Elbeland: posición y escala.
    perches: Vec<(Vec3, f32)>,
}

impl Boids {
    fn rand(&mut self) -> i32 {
        self.rng.rand()
    }
    fn chance(&mut self, n: i32) -> bool {
        self.rng.rand() % n == 0
    }
}

fn setup_boids(
    mut commands: Commands,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
) {
    let wa = args.world as i32 - 1;
    let data = &args.data;
    // `CMapManager::Load` (MapManager.cpp:49-200, 614-622).
    let main = match wa {
        0 => Some(("Object1", "Bird01.bmd")),
        1 | 4 => Some(("Object2", "Bat01.bmd")),
        3 => Some(("Object1", "Butterfly01.bmd")),
        51 => Some(("Object52", "typhoonall.bmd")),
        _ if in_blood_castle(wa) => Some(("Object12", "Crow01.bmd")),
        _ => None,
    };
    let mut models: Vec<Option<Model>> = (0..6).map(|_| None).collect();
    models[M_MAIN] = main.and_then(|(d, f)| load_model(data, d, f, &mut images));
    models[M_DRAGON] = load_model(data, "Monster", "Monster32.bmd", &mut images);
    if in_atlans(wa) {
        models[M_FISH2] = load_model(data, "Object8", "Fish02.bmd", &mut images);
        models[M_FISH2 + 1] = load_model(data, "Object8", "Fish03.bmd", &mut images);
    }
    if in_hellas(wa) {
        models[M_BIGMON] = load_model(data, "Monster", "Monster34.bmd", &mut images);
        if let Some(m) = &mut models[M_BIGMON] {
            m.hidden_mesh = 5;
        }
    }
    let mut perches = vec![];
    if wa == 51 {
        models[M_EAGLE] = load_model(data, "Object52", "sos3bi01.bmd", &mut images);
        perches = mu_data::objects::load(data, args.world)
            .map(|v| {
                v.into_iter().filter(|o| o.kind == EAGLE_PERCH).map(|o| (Vec3::from_array(o.position), o.scale)).collect()
            })
            .unwrap_or_default();
    }
    let list = (0..MAX_BOIDS).map(|_| Obj::new(crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material))).collect();
    commands.insert_resource(Boids {
        wa,
        models,
        list,
        rng: Rng::default(),
        event: Invasion::default(),
        tick: 0,
        perches,
    });
}

#[allow(clippy::too_many_arguments)]
fn step_boids(
    boids: Option<ResMut<Boids>>,
    terrain: Option<Res<LoadedTerrain>>,
    players: Query<&Player>,
    time: Res<Time<Fixed>>,
    invasion: Res<Invasion>,
    mut fire: ResMut<FireRequests>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut fx: ResMut<FxWorld>,
) {
    let (Some(mut b), Some(t), Ok(p)) = (boids, terrain, players.single()) else { return };
    let b = &mut *b;
    let t = &t.0;
    let fx = &mut *fx;
    b.tick = b.tick.wrapping_add(1);
    let world_ms = time.elapsed_secs() * 1000.0;
    let hero = Hero::of(p, t);
    let wa = b.wa;

    // Al empezar o acabar el evento, el original borra todos los bichos (`DeleteBoids`).
    if b.event != *invasion {
        b.event = *invasion;
        for o in &mut b.list {
            o.kill(fx);
        }
    }

    // El águila de Elbeland la suelta el objeto 62 al moverse (`MoveObjects` va antes que
    // `MoveBoids`), siempre en `Boids[1]`. El original solo mueve los objetos que se ven; aquí,
    // los que están tan cerca como los demás objetos con efectos.
    if wa == 51 && b.models[M_EAGLE].is_some() {
        for k in 0..b.perches.len() {
            let (pp, sc) = b.perches[k];
            if hero.range(pp) <= crate::fx::sim::OBJECT_FX_DISTANCE && !b.list[1].live {
                create_eagle(b, pp, sc);
            }
        }
    }

    for i in 0..boid_slots(wa) {
        // Kalima: 9 de cada 10 ticks no se crea nada.
        let create = !in_hellas(wa) || b.rand() % 10 == 0;
        if !b.list[i].live && create {
            if invasion.active() {
                // Un dragón 1 de cada 300 ticks por hueco libre.
                if b.models[M_DRAGON].is_some() && b.chance(300) {
                    spawn_invasion_dragon(b, i, hero.pos, invasion.0 == 3);
                }
            } else {
                create_boid(b, i, &hero, t, fx);
            }
            if !b.list[i].live {
                b.list[i].sync_anchor(fx);
                continue;
            }
        } else if !b.list[i].live {
            continue;
        }
        b.list[i].remember();

        if b.list[i].kind == Kind::InvasionDragon {
            step_invasion_dragon(b, i, t, &mut fire);
            // El rugido: 1 de cada 128 ticks por dragón vivo.
            if b.list[i].live && b.chance(128) {
                sound.play("SOUND_MONSTER_BULLATTACK1");
            }
            b.list[i].alpha_step();
            continue;
        }

        let kind = b.list[i].kind;
        // `PlayAnimation`: 0.5 los monstruos; en Elbeland, el `PlaySpeed` de su acción
        // (tornado 0.1, águila 0.5: MapManager.cpp:1182-1185).
        let speed = match kind {
            Kind::HeavenDragon | Kind::BigMon => 0.5,
            Kind::Tornado => 0.1,
            Kind::Eagle => 0.5,
            _ => 1.0,
        };
        play_animation(&mut b.list[i], &b.models, speed);

        match kind {
            Kind::Bat => {
                // `MoveBat`.
                let o = &mut b.list[i];
                o.pos.z = ground(t, o.pos) - o.timer.sin().abs() * 150.0 + 350.0;
                o.timer += 0.2;
            }
            Kind::Butterfly => move_butterfly(b, i, t),
            Kind::Bird | Kind::Crow => move_bird(b, i, &hero, t, world_ms),
            Kind::HeavenBug => move_heaven_bug(b, i, &hero),
            Kind::BigMon => {
                // `MoveBigMon` (GMHellas.cpp:668).
                let flip = b.chance(5);
                let o = &mut b.list[i];
                o.angle.z += o.gravity;
                if flip {
                    o.gravity = -o.gravity;
                }
                if o.life < 20 {
                    o.alpha /= 1.2;
                    o.velocity += 0.5;
                    o.angle.x += 2.0;
                }
                if o.life < 0 {
                    o.live = false;
                }
            }
            Kind::Eagle => move_eagle(b, i, world_ms),
            Kind::Tornado => {
                // `MoveTornado`.
                let turn = b.chance(500);
                let h = (b.rand() % 314) as f32 / 100.0;
                let o = &mut b.list[i];
                o.scale = 1.0;
                if turn {
                    o.head_angle.x = h;
                }
                o.pos.x += o.head_angle.x.sin() * 2.0;
                o.pos.y += o.head_angle.x.cos() * 2.0;
                o.angle.z = 0.0;
                if o.blend_mesh_light < 1.0 {
                    o.blend_mesh_light += 0.1;
                }
            }
            _ => {}
        }

        move_boid_group(b, i, &hero);

        // Atlans: con el personaje en la zona segura, los peces dan media vuelta y se van.
        {
            let o = &mut b.list[i];
            if o.life <= 0 && in_atlans(wa) && hero.wall == attr::SAFE_ZONE {
                o.angle.z += 180.0;
                if o.angle.z >= 360.0 {
                    o.angle.z -= 360.0;
                }
                o.life = 10;
                o.sub_type += 1;
            }
            if !matches!(kind, Kind::Eagle | Kind::Tornado) && o.sub_type >= 2 {
                o.live = false;
            }
            o.life -= 1;
        }

        // Sus sonidos, a menos de 6 casillas del personaje.
        if hero.range(b.list[i].pos) < 600.0 {
            match kind {
                Kind::Bird => {
                    if b.chance(512) {
                        sound.play_object("SOUND_BIRD01");
                    }
                    if b.chance(512) {
                        sound.play_object("SOUND_BIRD02");
                    }
                }
                Kind::Bat => {
                    if b.chance(256) {
                        sound.play_object("SOUND_BAT01");
                    }
                }
                Kind::Crow => {
                    if hero.wall == attr::SAFE_ZONE && b.chance(128) {
                        sound.play_object("SOUND_CROW");
                    }
                }
                _ => {}
            }
        }
        b.list[i].alpha_step();

        // El tornado echa humo cada vez que se dibuja (`CreateParticle(BITMAP_CLOUD, ..., 18, o)`).
        if kind == Kind::Tornado && b.list[i].live {
            let o = &mut b.list[i];
            let target = match o.anchor {
                Some(id) => Target::Effect(id),
                None => spawn_anchor(o, fx),
            };
            let angle = o.angle + Vec3::new(0.0, 0.0, 90.0);
            let particle = fxc::Particle::new(fxc::BITMAP_CLOUD, o.pos, angle, o.light, 18, o.scale, Some(target));
            crate::fx::request(fx, Spawn::Particle(particle));
        }
        b.list[i].sync_anchor(fx);
    }
}

/// La creación de `MoveBoids` (GOBoid.cpp:1303-1382) para el hueco `i` si su mapa la tiene.
fn create_boid(b: &mut Boids, i: usize, hero: &Hero, t: &Terrain, fx: &mut FxWorld) {
    let wa = b.wa;
    let can = match wa {
        0 | 1 | 3 | 4 | 10 => true,
        7 | 67 => open_wall(hero.wall),
        51 => i < 1 && b.chance(500) && !hero.safe,
        _ => in_blood_castle(wa) || in_hellas(wa),
    };
    if !can {
        return;
    }
    let o = &mut b.list[i];
    o.initialize();
    o.live = true;
    o.velocity = 1.0;
    o.light_enable = true;
    o.life = 0;
    o.sub_type = 0;
    o.light = Vec3::splat(0.5);
    o.alpha = 0.0;
    o.alpha_target = 1.0;
    o.gravity = 13.0;
    o.ai = Ai::Fly;
    o.action = 0;
    o.shadow_on = wa != 10;
    o.model = Some(M_MAIN);

    let done = match wa {
        0 => {
            o.kind = Kind::Bird;
            false
        }
        1 | 4 => {
            o.kind = Kind::Bat;
            false
        }
        3 => {
            o.kind = Kind::Butterfly;
            o.velocity = 0.3;
            o.light_enable = false;
            o.light = Vec3::ONE;
            false
        }
        51 => {
            o.kind = Kind::Tornado;
            o.velocity = 0.0;
            o.blend_mesh_light = 0.0;
            o.head_angle.x = (b.rng.rand() % 314) as f32 / 100.0;
            // (`Position[1] += 5` y luego se vuelve a colocar abajo)
            o.shadow_on = false;
            false
        }
        _ if in_blood_castle(wa) => {
            o.kind = Kind::Crow;
            false
        }
        10 => {
            create_dragon(b, i, hero, fx);
            true
        }
        7 | 67 => {
            create_atlans_fish(b, i, hero, t);
            true
        }
        _ => {
            create_big_mon(b, i, hero);
            true
        }
    };
    if !done {
        let (timer, rx, ry, rz) = (b.rand() % 314, b.rand() % 1024 - 512, b.rand() % 1024 - 512, b.rand() % 200 + 150);
        let o = &mut b.list[i];
        o.alpha_enable = true;
        o.scale = 0.8;
        o.timer = timer as f32 * 0.01;
        let mut p = hero.pos + Vec3::new(rx as f32, ry as f32, 0.0);
        p.z = ground(t, p) + rz as f32;
        o.angle = Vec3::ZERO;
        o.place(p);
    }
    let o = &mut b.list[i];
    if o.model.is_some_and(|m| b.models[m].is_none()) {
        o.live = false;
    }
}

/// `CreateDragon` (GOBoid.cpp:818): 3 dragones muy abajo y el resto bichos con estela.
fn create_dragon(b: &mut Boids, i: usize, hero: &Hero, fx: &mut FxWorld) {
    if i < 3 {
        let (scale, vel, grav, ang, rx, ry) =
            (b.rand() % 3 + 6, b.rand() % 10 + 10, b.rand() % 10 + 10, b.rand() % 360, b.rand() % 4000 - 2000, b.rand() % 4000 - 2000);
        let o = &mut b.list[i];
        o.kind = Kind::HeavenDragon;
        o.model = Some(M_DRAGON);
        o.scale = scale as f32 * 0.05;
        o.alpha = 1.0;
        o.alpha_target = 1.0;
        o.velocity = vel as f32 * 0.02;
        o.gravity = grav as f32 * 0.05;
        o.light_enable = true;
        o.alpha_enable = false;
        o.sub_type = 0;
        o.blend_mesh = -1;
        o.action = DRAGON_ACTION;
        o.frame = 0.0;
        o.angle = Vec3::new(0.0, 0.0, ang as f32);
        o.place(hero.pos + Vec3::new(rx as f32, ry as f32, -600.0));
    } else {
        let (timer, rx, ry) = (b.rand() % 314, b.rand() % 1024 - 512, b.rand() % 1024 - 512);
        let o = &mut b.list[i];
        o.kind = Kind::HeavenBug;
        o.model = None;
        o.velocity = 2.2;
        o.light_enable = false;
        o.life = 240 * 40;
        o.ai = Ai::Ground;
        o.light = Vec3::ONE;
        o.alpha_enable = true;
        o.scale = 0.8;
        o.blend_mesh = -1;
        o.timer = timer as f32 * 0.01;
        o.place(hero.pos + Vec3::new(rx as f32, ry as f32, 0.0));
        // (en Blood Castle sería el subtipo 3; `CreateDragon` solo crea en Icarus)
        joint_on(o, fx, fxc::MODEL_SPEARSKILL, 1, 25.0);
        let ang = b.rand() % 360;
        let o = &mut b.list[i];
        o.angle = Vec3::new(0.0, 0.0, ang as f32);
        o.prev_angle = o.angle;
    }
}

/// `CreateAtlanseFish` (GOBoid.cpp:880): solo con el personaje en la mitad sur (y < 128).
fn create_atlans_fish(b: &mut Boids, i: usize, hero: &Hero, t: &Terrain) {
    if hero.pos.y * 0.01 >= 128.0 {
        b.list[i].live = false;
        return;
    }
    let (ty, timer, slow, rx, ry, rz) =
        (b.rand() % 2, b.rand() % 314, b.rand() % 100 >= 90, b.rand() % 1024 - 512, b.rand() % 1024 - 512, b.rand() % 200 + 150);
    let o = &mut b.list[i];
    o.kind = Kind::AtlansFish;
    o.model = Some(M_FISH2 + ty as usize);
    o.gravity = 15.0;
    o.alpha_enable = true;
    o.scale = 0.8;
    o.blend_mesh = -1;
    o.timer = timer as f32 * 0.01;
    o.velocity = if slow { 0.25 } else { 0.3 };
    o.light_enable = false;
    o.angle = Vec3::ZERO;
    o.light = Vec3::ONE;
    let mut p = hero.pos + Vec3::new(rx as f32, ry as f32, 0.0);
    p.z = ground(t, p) + rz as f32;
    o.place(p);
}

/// `CreateBigMon` (GMHellas.cpp:633): solo cuando `MoveSceneFrame % 100 == 0`.
fn create_big_mon(b: &mut Boids, i: usize, hero: &Hero) {
    if b.tick % 100 != 0 {
        b.list[i].live = false;
        return;
    }
    let (scale, vel, grav, ang, rx, ry) =
        (b.rand() % 3 + 6, b.rand() % 10 + 10, b.rand() % 3, b.rand() % 30, b.rand() % 200, b.rand() % 200);
    let o = &mut b.list[i];
    o.kind = Kind::BigMon;
    o.model = Some(M_BIGMON);
    o.scale = 2.5 + scale as f32 * 0.05;
    o.alpha = 1.0;
    o.alpha_target = 1.0;
    o.velocity = vel as f32 * 0.04;
    o.gravity = grav as f32 - 1.5;
    o.light_enable = true;
    o.alpha_enable = false;
    o.sub_type = 0;
    o.blend_mesh = -1;
    o.life = 200;
    o.action = MONSTER_WALK;
    o.frame = 0.0;
    o.angle = Vec3::new(0.0, 0.0, 90.0 - ang as f32 - 15.0);
    o.place(hero.pos + Vec3::new(-1000.0 - rx as f32, -500.0 + ry as f32, -800.0));
}

/// El águila de Elbeland (GMNewTown.cpp:197-231). Como el original, no se llama a
/// `Initialize`: `AI` y `HeadAngle` siguen como los dejó la anterior.
fn create_eagle(b: &mut Boids, pos: Vec3, object_scale: f32) {
    let timer = b.rand() % 314;
    let o = &mut b.list[1];
    o.live = true;
    o.kind = Kind::Eagle;
    o.model = Some(M_EAGLE);
    o.velocity = 0.0;
    o.light_enable = true;
    o.life = 0;
    o.sub_type = 1;
    o.light = Vec3::splat(0.5);
    o.alpha = 0.0;
    o.alpha_target = 1.0;
    o.gravity = 10.0 * object_scale;
    o.alpha_enable = true;
    o.scale = 0.5;
    // `ShadowScale` 15 si el objeto está alto; si no, 0 (sombra transparente).
    o.shadow_on = pos.z > 100.0;
    o.blend_mesh = -1;
    o.timer = timer as f32 * 0.01;
    o.angle = Vec3::ZERO;
    o.action = 0;
    o.place(pos);
}

/// `MoveButterFly` (GOBoid.cpp:926).
fn move_butterfly(b: &mut Boids, i: usize, t: &Terrain) {
    let turn = b.chance(32);
    let (ra, rd) = if turn { (b.rand() % 360, b.rand() % 15 - 7) } else { (0, 0) };
    let (rd2, rz) = (b.rand() % 15 - 7, b.rand() % 15 - 7);
    let o = &mut b.list[i];
    if turn {
        o.angle.z = ra as f32;
        o.dir.z = rd as f32;
    }
    o.dir.z += rd2 as f32 * 0.2;
    let h = ground(t, o.pos);
    if o.pos.z < h + 50.0 {
        o.dir.z = o.dir.z * 0.8 + 1.0;
    }
    if o.pos.z > h + 300.0 {
        o.dir.z = o.dir.z * 0.8 - 1.0;
    }
    o.pos.z += rz as f32 * 0.3;
}

/// `MoveBird` (GOBoid.cpp:948), con la bajada a posarse de MuMain: solo con el personaje
/// quieto, y vuelven a volar cuando se mueve. Pájaros de Lorencia y cuervos de Blood Castle.
fn move_bird(b: &mut Boids, i: usize, hero: &Hero, t: &Terrain, world_ms: f32) {
    let r = [b.rand() % 16 - 8, b.rand() % 256, b.rand() % 16 - 8];
    let o = &mut b.list[i];
    if o.ai == Ai::Fly {
        let dist = hero.range(o.pos);
        // Solo bajan a posarse si el personaje está quieto: si no, rebotan en el
        // suelo y vuelven a bajar una y otra vez a su lado (parece que lo persiguen).
        if hero.still && (world_ms as i32) % 8192 < 2048 && (200.0..=400.0).contains(&dist) {
            o.ai = Ai::Down;
        }
        o.velocity = 1.0;
        o.pos.z += r[0] as f32;
        if o.pos.z < 200.0 {
            o.dir.z = 10.0;
        } else if o.pos.z > 600.0 {
            o.dir.z = -10.0;
        }
    }
    if o.ai == Ai::Down {
        o.dir.z = -20.0;
        let h = ground(t, o.pos);
        if o.pos.z < h {
            let can_land = wall_at(t, o.pos.x, o.pos.y) & attr::NO_MOVE == 0;
            if can_land && hero.still {
                o.pos.z = h;
                o.ai = Ai::Ground;
                o.action = BIRD_GROUND;
            } else {
                take_off(o);
            }
        }
    }
    if o.ai == Ai::Ground && (!hero.still || r[1] == 0) {
        take_off(o);
    }
    if o.ai == Ai::Up {
        o.pos.z += r[2] as f32;
        o.velocity -= 0.005;
        if o.velocity <= 1.0 {
            o.ai = Ai::Fly;
        }
    }
}

fn take_off(o: &mut Obj) {
    o.ai = Ai::Up;
    o.velocity = 1.1;
    o.dir.z = 20.0;
    o.action = BIRD_FLY;
}

/// `MoveHeavenBug` (GOBoid.cpp:1003): el ángulo se usa como radianes, como en el original.
fn move_heaven_bug(b: &mut Boids, i: usize, hero: &Hero) {
    let frame = b.tick as f32;
    let gone = b.chance(5120);
    let o = &mut b.list[i];
    o.pos.x += o.velocity * o.angle.z.sin();
    o.pos.y -= o.velocity * o.angle.z.cos();
    let k = i as f32;
    o.angle.z += 0.01 * ((34571.0 + frame + k * 41273.0) * 0.0003).cos() * ((17732.0 + frame + k * 5161.0) * 0.0003).sin();
    if hero.range(o.pos) >= 1500.0 || gone {
        o.live = false;
    }
}

/// `MoveEagle` (GOBoid.cpp:1028).
fn move_eagle(b: &mut Boids, i: usize, world_ms: f32) {
    let start = b.list[i].sub_type == 0 && b.chance(120);
    let o = &mut b.list[i];
    if start {
        o.sub_type = 1;
        o.frame = 0.0;
    } else if o.sub_type == 1 && o.frame == 24.0 {
        o.sub_type = 2;
    } else if o.sub_type == 2 && o.frame == 0.0 {
        o.sub_type = 0;
    } else if o.sub_type == 0 {
        o.frame = 0.0;
    }
    let seed = world_ms * 0.001;
    let range = o.gravity;
    let p = o.pos;
    let heading = |h: Vec3| fxc::create_angle(p.x, p.y, p.x + h.x, p.y + h.y);
    let mut fangle = 0.0;
    let h = &mut o.head_angle;
    if o.ai == Ai::Fly {
        h.x = seed.cos() * range;
        h.y = seed.sin() * range;
        fangle = heading(*h);
        if h.z == 0.0 && h.x > h.y {
            h.z = 1.0;
        } else if h.z == 1.0 && h.x < h.y {
            h.z = 2.0;
        } else if h.z == 2.0 && h.x > h.y {
            o.ai = Ai::Ground;
            h.z = 0.0;
            h.x = seed.sin() * range;
            h.y = seed.cos() * range;
            fangle = heading(*h);
        }
    } else if o.ai == Ai::Ground {
        h.x = seed.sin() * range;
        h.y = seed.cos() * range;
        fangle = heading(*h);
        if h.z == 0.0 && h.x < h.y {
            h.z = 1.0;
        } else if h.z == 1.0 && h.x > h.y {
            h.z = 2.0;
        } else if h.z == 2.0 && h.x < h.y {
            o.ai = Ai::Fly;
            h.z = 0.0;
            h.x = seed.cos() * range;
            h.y = seed.sin() * range;
            fangle = heading(*h);
        }
    }
    o.pos.x += o.head_angle.x;
    o.pos.y += o.head_angle.y;
    o.pos.z += (world_ms * 0.0005).sin();
    o.angle.y += (world_ms * 0.001).sin() * 0.4;
    o.angle.z = fangle + 270.0;
}

/// `MoveBoidGroup` (GOBoid.cpp:1120).
fn move_boid_group(b: &mut Boids, i: usize, hero: &Hero) {
    if b.list[i].ai == Ai::Ground {
        return;
    }
    if b.list[i].kind != Kind::Butterfly || b.chance(4) {
        move_boid(&mut b.list, i);
    }
    let d = if in_atlans(b.wa) {
        let timer = b.list[i].timer;
        let (r, g) = if timer < 5.0 {
            (if i < 35 { b.rand() % 16 + 8 } else { b.rand() % 16 + 16 }, 15.0)
        } else {
            (b.rand() % 32 + 32, 5.0)
        };
        let o = &mut b.list[i];
        o.gravity = g;
        o.timer += 0.1;
        if o.timer >= 10.0 {
            o.timer = 0.0;
        }
        Vec3::new(o.velocity * r as f32, 0.0, o.dir.z)
    } else {
        let o = &b.list[i];
        Vec3::new(o.velocity * 25.0, 0.0, o.dir.z)
    };
    let kill = !matches!(b.list[i].kind, Kind::HeavenDragon | Kind::BigMon) && b.wa != 51 && b.chance(512);
    let o = &mut b.list[i];
    let p = fxc::rotate(o.angle, d);
    o.pos += p;
    o.dir.x = o.pos.x + 3.0 * p.x;
    o.dir.y = o.pos.y + 3.0 * p.y;
    let fly_distance = match o.kind {
        Kind::HeavenDragon => 4000.0,
        Kind::BigMon => 3000.0,
        _ => 1500.0,
    };
    if kill || hero.range(o.pos) >= fly_distance {
        o.live = false;
    }
}

fn spawn_invasion_dragon(b: &mut Boids, i: usize, hero: Vec3, golden: bool) {
    let (scale, life, timer) = (b.rand() % 3 + 6, b.rand() % 128, b.rand() % 10);
    let (rx, ry) = (b.rand() % 600 - 100, b.rand() % 400 + 200);
    let o = &mut b.list[i];
    o.initialize();
    o.live = true;
    o.kind = Kind::InvasionDragon;
    o.model = Some(M_DRAGON);
    o.golden = golden;
    o.scale = scale as f32 * 0.1;
    o.alpha = 1.0;
    o.alpha_target = 1.0;
    o.velocity = 0.5;
    o.light_enable = true;
    o.light = Vec3::splat(0.5);
    o.alpha_enable = false;
    o.life = 128 + life;
    o.timer = timer as f32 * 0.1;
    o.angle = Vec3::new(0.0, 0.0, -90.0);
    o.action = DRAGON_ACTION;
    o.frame = 0.0;
    o.shadow_on = true;
    o.place(hero + Vec3::new(rx as f32, ry as f32, 300.0));
}

/// Dragón de la invasión (GOBoid.cpp:1395-1413): avanza `escala·40` por tick hacia donde mira,
/// entre 300 y 400 sobre el suelo, con fuego en la cabeza; vive 128-255 ticks.
fn step_invasion_dragon(b: &mut Boids, i: usize, t: &Terrain, fire: &mut FireRequests) {
    play_animation(&mut b.list[i], &b.models, 0.5);
    let head = b.models[M_DRAGON].as_ref().map(|m| m.bmd.clone());
    let o = &mut b.list[i];
    o.pos += fxc::rotate(o.angle, Vec3::new(o.scale * 40.0, 0.0, 0.0));
    o.pos.z = ground(t, o.pos) + 300.0 - o.timer.sin().abs() * 100.0 + 100.0;
    o.timer += o.scale * 0.05;
    o.life -= 1;
    if o.life <= 0 {
        o.live = false;
        return;
    }
    // Fuego en la cabeza (hueso 11, desplazado 50 hacia −Y), en la pose que se dibuja.
    if let Some(model) = head {
        let bones = model.bone_matrices_at(DRAGON_ACTION, o.frame);
        if let Some(m) = bones.get(DRAGON_HEAD_BONE) {
            let local = Vec3::from_array(bmd::transform_point(m, [0.0, -50.0, 0.0]));
            let rot = Quat::from_rotation_z((o.angle.z + 90.0).to_radians());
            fire.0.push((o.pos + rot * (local * o.scale), Vec3::ONE));
        }
    }
}

fn draw_boids(
    mut commands: Commands,
    boids: Option<ResMut<Boids>>,
    fixed: Res<Time<Fixed>>,
    terrain: Option<Res<LoadedTerrain>>,
    mut q: Query<(&mut Transform, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut sprites: ResMut<BoidSprites>,
) {
    sprites.0.clear();
    let Some(mut b) = boids else { return };
    let b = &mut *b;
    let a = fixed.overstep_fraction();
    draw_objs(
        &mut b.list,
        &b.models,
        a,
        terrain.as_deref().map(|t| &t.0),
        &mut commands,
        &mut q,
        &mut meshes,
        &mut materials,
        &mut sprites.0,
        &mut b.rng,
        "Bicho",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calc_angle_como_el_original() {
        // 0 = +X, 90 = +Y (lo que mueve `AngleMatrix(0, 0, a)` · (v, 0, 0)).
        assert_eq!(calc_angle(0.0, 0.0, 10.0, 0.0), 0);
        assert_eq!(calc_angle(0.0, 0.0, 10.0, 10.0), 45);
        assert_eq!(calc_angle(0.0, 0.0, -10.0, 0.0), 180);
        assert_eq!(calc_angle(0.0, 0.0, 10.0, -10.0), 315);
        assert_eq!(calc_angle(0.0, 0.0, -10.0, 10.0), 135);
        // Con dx = 0 el original da 0 aunque el objetivo esté en ±Y.
        assert_eq!(calc_angle(0.0, 0.0, 0.0, 10.0), 0);
    }

    #[test]
    fn turn_angle_gira_por_el_lado_corto() {
        assert_eq!(turn_angle_int(10, 30, 13), 23);
        assert_eq!(turn_angle_int(30, 10, 13), 17);
        assert_eq!(turn_angle_int(350, 10, 13), 3);
        assert_eq!(turn_angle_int(10, 350, 13), 357);
        assert_eq!(turn_angle_int(10, 15, 13), 15);
        // Sin giro (los dragones de Icarus, `(int)Gravity` = 0).
        assert_eq!(turn_angle_int(100, 200, 0), 100);
    }

    #[test]
    fn indice_de_casilla_da_la_vuelta() {
        assert_eq!(tile_index(150.0, 250.0), 2 * 256 + 1);
        assert_eq!(tile_index(25700.0, 0.0), 1);
        // (int)(-150 / 100) = -1 → & 255 = 255.
        assert_eq!(tile_index(-150.0, 0.0), 255);
    }

    #[test]
    fn alfa_sube_a_pasos() {
        let mut o = Obj::new(crate::shadow::Shadow(Handle::default(), Entity::PLACEHOLDER));
        o.alpha_enable = true;
        o.alpha_target = 0.3;
        for _ in 0..20 {
            o.alpha_step();
        }
        assert!((o.alpha - 0.3).abs() < 0.051);
        o.alpha_enable = false;
        o.alpha_target = 1.0;
        o.alpha = 0.0;
        o.alpha_step();
        assert!((o.alpha - 0.1).abs() < 1e-6);
    }

    #[test]
    fn huecos_por_mapa() {
        assert_eq!(boid_slots(0), 5);
        assert_eq!(boid_slots(7), 40);
        assert_eq!(boid_slots(10), 13);
        assert_eq!(boid_slots(24), 2);
        assert_eq!(boid_slots(36), 2);
        assert_eq!(boid_slots(51), 2);
        assert!(in_blood_castle(11) && in_blood_castle(17) && in_blood_castle(52) && !in_blood_castle(18));
    }
}
