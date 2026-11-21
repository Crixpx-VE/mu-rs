//! Núcleo del motor de efectos: los mismos objetos que el original (`PARTICLE`, `JOINT`, el
//! `OBJECT` de los efectos y los sprites de un fotograma) y el contexto con el que se crean y se
//! mueven (`CreateParticle`, `CreateJoint`, `CreateEffect`, `CreateSprite`, `rand()`...).
//!
//! Reglas para traducir código de MuMain a este motor:
//! - Todo va en coordenadas de MU (Z arriba, 100 unidades por casilla) y ángulos en grados.
//! - La lógica corre a 25 ticks por segundo: `FPS_ANIMATION_FACTOR` = 1 y
//!   `rand_fps_check(n)` = [`Ctx::rand_fps_check`] (1 de cada n ticks). Las fórmulas se copian tal cual.
//! - `rand()` → [`Ctx::rand`]; `Random::RangeFloat/RangeInt(a, b)` → [`Ctx::range_f`] / [`Ctx::range_i`].
//! - `AngleMatrix(angle, m)` + `VectorRotate(v, m, out)` → `rotate(angle, v)`;
//!   `VectorIRotate` → `irotate(angle, v)`.
//! - `o->Target` / `o->Owner` (un personaje) → `Target::Actor`; consultar con [`Ctx::actor`].
//!   `b->TransformPosition(BoneTransform[i], p, out)` → [`Actor::bone_world`].
//! - `RequestTerrainHeight(x, y)` → [`Ctx::terrain_height`]; `AddTerrainLight` → [`Ctx::add_terrain_light`].
//! - `WorldTime` (ms) → [`Ctx::world_time`].

use std::sync::Arc;

use bevy::math::{EulerRot, Mat3, Quat, Vec2, Vec3};
use bevy::prelude::Entity;
use mu_data::bmd::{self, Mat34};
use mu_data::terrain::Terrain;

use crate::terrain::TerrainLights;

pub use super::tables::ids::*;
pub use super::material::Blend;

// --- Matemáticas del original -------------------------------------------------------------------

/// `AngleMatrix`: giro de MU con ángulos en grados (x, y, z) = Rz·Ry·Rx.
pub fn angle_matrix(angle: Vec3) -> Mat3 {
    let a = angle * (std::f32::consts::PI / 180.0);
    Mat3::from_quat(Quat::from_euler(EulerRot::ZYX, a.z, a.y, a.x))
}

/// `AngleMatrix(angle) + VectorRotate(v)`.
pub fn rotate(angle: Vec3, v: Vec3) -> Vec3 {
    angle_matrix(angle) * v
}

/// `AngleMatrix(angle) + VectorIRotate(v)` (giro inverso).
pub fn irotate(angle: Vec3, v: Vec3) -> Vec3 {
    angle_matrix(angle).transpose() * v
}

/// `CreateAngle(x1, y1, x2, y2)`: rumbo de MU en grados (0 = hacia -Y).
pub fn create_angle(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let (dx, dy) = (x2 - x1, y2 - y1);
    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return 0.0;
    }
    dx.atan2(-dy).to_degrees().rem_euclid(360.0)
}

/// `TurnAngle2(angle, target, step)`: gira hacia `target` como mucho `step` grados.
pub fn turn_angle(angle: f32, target: f32, step: f32) -> f32 {
    if step <= 0.0 {
        return angle.rem_euclid(360.0);
    }
    let delta = (target - angle + 540.0).rem_euclid(360.0) - 180.0;
    (angle + delta.clamp(-step, step)).rem_euclid(360.0)
}

// --- Azar ---------------------------------------------------------------------------------------

/// Generador del original: `rand()` devuelve 0..=32767 como el de MSVC.
#[derive(Clone)]
pub struct Rng(u32);

impl Default for Rng {
    fn default() -> Self {
        Self(0x1234_5678)
    }
}

impl Rng {
    pub fn rand(&mut self) -> i32 {
        // LCG de MSVC (mismo rango que el original).
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.0 >> 16) & 0x7FFF) as i32
    }
}

// --- Personajes y objetivos -----------------------------------------------------------------------

/// Foto de un personaje (o monstruo) en este tick, para los efectos que lo siguen.
#[derive(Clone)]
pub struct Actor {
    pub position: Vec3,
    /// Ángulos en grados (el del cuerpo es `angle.z`).
    pub angle: Vec3,
    pub scale: f32,
    /// Huesos del modelo en su pose actual (espacio del modelo).
    pub bones: Arc<Vec<Mat34>>,
    pub action: usize,
    pub frame: f32,
    /// Arma en la mano derecha: id del modelo (`MODEL_SWORD + n`...) y nivel (+0..+15).
    pub weapon: Option<(i32, u8)>,
    /// Hueso de la mano del arma (`LinkBone`).
    pub weapon_bone: usize,
    pub live: bool,
    /// `m_vPosSword`: punto que calcula la habilidad en curso (p. ej. Death Stab).
    pub pos_sword: Vec3,
}

impl Actor {
    /// `b->TransformPosition(BoneTransform[bone], offset, out)` con la posición, giro y escala del
    /// personaje: el punto `offset` (en ejes del hueso) en el mundo.
    pub fn bone_world(&self, bone: usize, offset: Vec3) -> Vec3 {
        let local = match self.bones.get(bone) {
            Some(m) => Vec3::from_array(bmd::transform_point(m, offset.to_array())),
            None => offset,
        };
        self.position + rotate(self.angle, local * self.scale)
    }
}

/// A quién sigue un efecto (`o->Target` / `o->Owner`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Actor(Entity),
    /// Otro efecto del motor, por su número único.
    Effect(u32),
}

// --- Objetos del motor ------------------------------------------------------------------------------

/// `PARTICLE` del original.
#[derive(Clone)]
pub struct Particle {
    pub ty: i32,
    pub tex_type: i32,
    pub sub_type: i32,
    pub scale: f32,
    pub position: Vec3,
    pub angle: Vec3,
    pub light: Vec3,
    pub alpha: f32,
    pub life_time: f32,
    pub target: Option<Target>,
    pub rotation: f32,
    pub frame: i32,
    pub enable_move: bool,
    pub gravity: f32,
    pub velocity: Vec3,
    pub turning_force: Vec3,
    pub start_position: Vec3,
    pub num_bone: i32,
    pub repeatedly: bool,
    pub repeatedly_height: f32,
    /// Estado del tick anterior, para interpolar al dibujar.
    pub prev_position: Vec3,
    pub prev_scale: f32,
    pub prev_light: Vec3,
    pub live: bool,
}

impl Particle {
    pub fn new(ty: i32, position: Vec3, angle: Vec3, light: Vec3, sub_type: i32, scale: f32, target: Option<Target>) -> Self {
        Self {
            ty,
            tex_type: ty,
            sub_type,
            scale,
            position,
            angle,
            light,
            alpha: 1.0,
            life_time: 2.0,
            target,
            rotation: 0.0,
            frame: 0,
            enable_move: true,
            gravity: 0.0,
            velocity: Vec3::ZERO,
            turning_force: Vec3::ZERO,
            start_position: position,
            num_bone: 0,
            repeatedly: false,
            repeatedly_height: 0.0,
            prev_position: position,
            prev_scale: scale,
            prev_light: light,
            live: true,
        }
    }
}

/// `RenderType` de las estelas.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum JointRender {
    /// `RENDER_TYPE_ALPHA_BLEND` → suma.
    #[default]
    AlphaBlend,
    /// `RENDER_TYPE_ALPHA_TEST`.
    AlphaTest,
    /// `RENDER_TYPE_ALPHA_BLEND_MINUS` → resta.
    AlphaBlendMinus,
    /// `RENDER_TYPE_ALPHA_BLEND_OTHER` → `Blend2`.
    AlphaBlendOther,
}

/// Cómo se desplaza la textura de una estela al dibujarla (casos especiales de `DescribeJoint`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum JointScroll {
    #[default]
    None,
    /// `BITMAP_JOINT_THUNDER`: U·2 − scroll en una cara y + scroll·2 en la otra.
    Thunder,
    /// `bTileMapping`: U = (NumTails − j) / 16, ×2, − 2·scroll.
    Tile,
    /// `BITMAP_FLARE_FORCE` (subtipos 0-4, 11-13): U − scroll.
    Flare,
}

pub const RENDER_FACE_ONE: u8 = 1;
pub const RENDER_FACE_TWO: u8 = 2;
pub const MAX_TAILS: usize = 200;

/// `JOINT` del original: una estela hecha de "colas" (cruces de 4 puntos) que se van dejando atrás.
#[derive(Clone)]
pub struct Joint {
    pub ty: i32,
    pub tex_type: i32,
    pub sub_type: i32,
    pub render_type: JointRender,
    pub render_face: u8,
    pub scale: f32,
    pub position: Vec3,
    pub start_position: Vec3,
    pub angle: Vec3,
    pub head_angle: Vec3,
    pub light: Vec3,
    pub target: Option<Target>,
    pub target_position: Vec3,
    pub only_one_render: u8,
    pub life_time: f32,
    pub collision: bool,
    pub velocity: f32,
    pub direction: Vec3,
    pub pk_key: i32,
    pub skill: i32,
    pub weapon: f32,
    pub multi_use: f32,
    pub tile_mapping: bool,
    pub reverse_uv: u8,
    pub create_tails: bool,
    pub max_tails: usize,
    /// Colas: la [0] es la más nueva. Cada una son 4 puntos (cruz alrededor de la posición).
    pub tails: Vec<[Vec3; 4]>,
    pub scroll: JointScroll,
    /// Contador libre para el código de cada tipo (lo que el original guardaba en campos sueltos).
    pub user: [f32; 4],
    pub live: bool,
}

impl Joint {
    pub fn new(ty: i32, position: Vec3, target_position: Vec3, angle: Vec3, sub_type: i32, target: Option<Target>, scale: f32) -> Self {
        Self {
            ty,
            tex_type: ty,
            sub_type,
            render_type: JointRender::AlphaBlend,
            render_face: RENDER_FACE_ONE | RENDER_FACE_TWO,
            scale,
            position,
            start_position: position,
            angle,
            head_angle: Vec3::ZERO,
            light: Vec3::ONE,
            target,
            target_position,
            only_one_render: 0,
            life_time: 0.0,
            collision: false,
            velocity: 0.0,
            direction: Vec3::ZERO,
            // CreateJoint tiene PKKey = -1 por defecto.
            pk_key: -1,
            skill: 0,
            weapon: 0.0,
            multi_use: 0.0,
            tile_mapping: false,
            reverse_uv: 0,
            create_tails: true,
            max_tails: 1,
            tails: Vec::new(),
            scroll: JointScroll::None,
            user: [0.0; 4],
            live: true,
        }
    }

    /// `CreateTail`/`SetNewestTail`: una cruz de 4 puntos de ancho `scale` girada con `angle`.
    pub fn make_tail(&self) -> [Vec3; 4] {
        let m = angle_matrix(self.angle);
        let h = self.scale * 0.5;
        [
            self.position + m * Vec3::new(-h, 0.0, 0.0),
            self.position + m * Vec3::new(h, 0.0, 0.0),
            self.position + m * Vec3::new(0.0, 0.0, -h),
            self.position + m * Vec3::new(0.0, 0.0, h),
        ]
    }

    /// `FollowTail`: las colas se desplazan una posición y se deja una nueva delante.
    pub fn follow_tail(&mut self) {
        let tail = self.make_tail();
        self.tails.insert(0, tail);
        let max = self.max_tails.max(1).min(MAX_TAILS);
        self.tails.truncate(max);
    }
}

/// El `OBJECT` de los efectos (`Effects[]`): normalmente un modelo `.bmd` que se mueve.
#[derive(Clone)]
pub struct Effect {
    /// Número único (para que otros efectos lo sigan con `Target::Effect`).
    pub id: u32,
    pub ty: i32,
    pub sub_type: i32,
    pub position: Vec3,
    pub start_position: Vec3,
    pub angle: Vec3,
    pub head_angle: Vec3,
    pub direction: Vec3,
    pub light: Vec3,
    pub alpha: f32,
    pub alpha_target: f32,
    pub scale: f32,
    pub life_time: f32,
    /// Velocidad de la animación del modelo (fotogramas por tick).
    pub velocity: f32,
    pub gravity: f32,
    pub distance: f32,
    pub timer: f32,
    pub animation_frame: f32,
    pub current_action: usize,
    pub blend_mesh: i32,
    pub blend_mesh_light: f32,
    pub blend_mesh_tex_coord: Vec2,
    pub hidden_mesh: i32,
    pub light_enable: bool,
    pub owner: Option<Target>,
    pub target: Option<Target>,
    pub pk_key: i32,
    /// `Kind` del original (el argumento `Skill` de `CreateEffect`).
    pub kind: i32,
    /// `Skill` del original (el argumento `SkillIndex`).
    pub skill: i32,
    pub weapon: i32,
    pub weapon_level: i32,
    pub ai: i32,
    pub live: bool,
    /// Se dibuja con el modelo de su tipo (si `false`, solo emite partículas o se dibuja a mano).
    pub visible: bool,
    /// Estado del tick anterior, para interpolar.
    pub prev_position: Vec3,
    pub prev_angle: Vec3,
    /// Campos libres para lo que cada tipo necesite guardar.
    pub user: [f32; 8],
}

impl Effect {
    pub fn new(id: u32, ty: i32, position: Vec3, angle: Vec3, light: Vec3, sub_type: i32, owner: Option<Target>, pk_key: i32, kind: i32, skill: i32, scale: f32) -> Self {
        Self {
            id,
            ty,
            sub_type,
            position,
            start_position: position,
            angle,
            head_angle: Vec3::ZERO,
            direction: Vec3::ZERO,
            light,
            alpha: 1.0,
            alpha_target: 1.0,
            scale: if scale <= 0.0 { 0.9 } else { scale },
            life_time: 0.0,
            velocity: 0.3,
            gravity: 0.0,
            distance: 0.0,
            timer: 0.0,
            animation_frame: 0.0,
            current_action: 0,
            blend_mesh: -1,
            blend_mesh_light: 1.0,
            blend_mesh_tex_coord: Vec2::ZERO,
            hidden_mesh: -1,
            light_enable: true,
            owner,
            target: None,
            pk_key,
            kind,
            skill,
            weapon: 0,
            weapon_level: 0,
            ai: 0,
            live: true,
            visible: true,
            prev_position: position,
            prev_angle: angle,
            user: [0.0; 8],
        }
    }
}

/// `CreateSprite`: un cuadrado mirando a la cámara que dura un tick.
#[derive(Clone, Copy)]
pub struct SpriteFx {
    pub ty: i32,
    pub position: Vec3,
    pub scale: f32,
    pub light: Vec3,
    pub rotation: f32,
    /// 0 suma, 1 resta, 2 prueba de alfa, 3 `Blend2` (`DescribeSprite`).
    pub sub_type: i32,
    pub alpha: f32,
}

// --- Habilidades -----------------------------------------------------------------------------------

/// `g_iLimitAttackTime`: cuando `AttackTime` llega aquí se lanza el efecto final de la habilidad.
pub const LIMIT_ATTACK_TIME: i32 = 15;

/// Una habilidad en curso de un personaje: lo que el original guarda en el `CHARACTER`
/// (`Skill`, `AttackTime`, `TargetCharacter`, `m_vPosSword`...). Cada tick: `AttackTime += 1`,
/// se llama a `stage` (`AttackStage`/`AttackEffect`) y al llegar a [`LIMIT_ATTACK_TIME`] a
/// `finish` (el `switch` de `ZzzCharacter.cpp` ≈ 4154-4437); entonces termina.
#[derive(Clone)]
pub struct Cast {
    pub owner: Entity,
    pub skill: i32,
    pub attack_time: i32,
    /// Objetivo (`TargetCharacter`) y su posición (o el punto del suelo elegido).
    pub target: Option<Target>,
    pub target_position: Vec3,
    /// `m_vPosSword` y otros datos que el original guarda en el personaje.
    pub pos_sword: Vec3,
    pub user: [f32; 8],
    pub live: bool,
}

impl Cast {
    pub fn new(owner: Entity, skill: i32, target: Option<Target>, target_position: Vec3) -> Self {
        Self { owner, skill, attack_time: 1, target, target_position, pos_sword: Vec3::ZERO, user: [0.0; 8], live: true }
    }
}

pub type SkillStage = fn(&mut Cast, &mut Ctx);

#[derive(Clone, Copy, Default)]
pub struct SkillKind {
    /// Cada tick mientras dura (`AttackStage` + `AttackEffect`).
    pub stage: Option<SkillStage>,
    /// Al llegar `AttackTime` a [`LIMIT_ATTACK_TIME`].
    pub finish: Option<SkillStage>,
}

// --- Peticiones de creación ---------------------------------------------------------------------

/// Lo que se pide crear durante un tick; se crea al acabar el tick (con su función de creación).
pub enum Spawn {
    Particle(Particle),
    Joint(Joint),
    Effect(Effect),
}

// --- Contexto --------------------------------------------------------------------------------------

/// Lo que ven las funciones de cada tipo de efecto mientras se crean o se mueven.
pub struct Ctx<'a> {
    pub rng: &'a mut Rng,
    /// `WorldTime`: milisegundos desde que arrancó el juego.
    pub world_time: f32,
    pub terrain: Option<&'a Terrain>,
    pub hero: Option<Entity>,
    pub actors: &'a std::collections::HashMap<Entity, Actor>,
    /// Posición de los efectos vivos al empezar el tick (para `Target::Effect`).
    pub effect_positions: &'a std::collections::HashMap<u32, Vec3>,
    pub spawns: &'a mut Vec<Spawn>,
    pub sprites: &'a mut Vec<SpriteFx>,
    pub lights: &'a mut TerrainLights,
    /// `EarthQuake`: temblor de cámara (se suma a la inclinación).
    pub earthquake: &'a mut f32,
    pub next_effect_id: &'a mut u32,
    /// `gMapManager.WorldActive`: mapa actual con la numeración del original (Lorencia = 0).
    pub world_active: i32,
}

impl Ctx<'_> {
    pub fn rand(&mut self) -> i32 {
        self.rng.rand()
    }

    /// `rand_fps_check(n)`: verdadero 1 de cada `n` ticks (siempre si n ≤ 1).
    pub fn rand_fps_check(&mut self, n: i32) -> bool {
        n <= 1 || self.rand() % n == 0
    }

    /// `Random::RangeFloat(a, b)` (ambos incluidos).
    pub fn range_f(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * (self.rand() as f32 / 32767.0)
    }

    /// `Random::RangeInt(a, b)` (ambos incluidos).
    pub fn range_i(&mut self, a: i32, b: i32) -> i32 {
        if b <= a {
            return a;
        }
        a + self.rand() % (b - a + 1)
    }

    pub fn world_time(&self) -> f32 {
        self.world_time
    }

    /// `RequestTerrainHeight(x, y)` en unidades de MU.
    pub fn terrain_height(&self, x: f32, y: f32) -> f32 {
        self.terrain.map_or(0.0, |t| t.height_at(x / 100.0, y / 100.0))
    }

    /// `TerrainWall[...]` (atributos de la casilla) en una posición de MU.
    pub fn terrain_attribute(&self, x: f32, y: f32) -> u16 {
        let (tx, ty) = ((x / 100.0).max(0.0) as usize, (y / 100.0).max(0.0) as usize);
        self.terrain.map_or(0, |t| if tx < 256 && ty < 256 { t.attribute(tx, ty) } else { 0 })
    }

    pub fn actor(&self, t: Option<Target>) -> Option<&Actor> {
        match t? {
            Target::Actor(e) => self.actors.get(&e),
            Target::Effect(_) => None,
        }
    }

    pub fn hero_actor(&self) -> Option<&Actor> {
        self.actors.get(&self.hero?)
    }

    /// Posición de lo que sigue (personaje o efecto).
    pub fn target_position(&self, t: Option<Target>) -> Option<Vec3> {
        match t? {
            Target::Actor(e) => self.actors.get(&e).map(|a| a.position),
            Target::Effect(id) => self.effect_positions.get(&id).copied(),
        }
    }

    /// `AddTerrainLight(x, y, light, range, PrimaryTerrainLight)`: x, y en MU, radio en casillas.
    pub fn add_terrain_light(&mut self, x: f32, y: f32, light: Vec3, range: f32) {
        self.lights.add(Vec2::new(x / 100.0, y / 100.0), light.to_array(), range);
    }

    /// `SetEarthQuake`: el temblor más fuerte del tick.
    pub fn set_earthquake(&mut self, amount: f32) {
        if amount.abs() > self.earthquake.abs() {
            *self.earthquake = amount;
        }
    }

    /// `CreateParticle(Type, Position, Angle, Light, SubType, Scale, Owner)`.
    pub fn create_particle(&mut self, ty: i32, position: Vec3, angle: Vec3, light: Vec3, sub_type: i32, scale: f32, owner: Option<Target>) {
        self.spawns.push(Spawn::Particle(Particle::new(ty, position, angle, light, sub_type, scale, owner)));
    }

    /// `CreateJoint(Type, Position, TargetPosition, Angle, SubType, Target, Scale)`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_joint(&mut self, ty: i32, position: Vec3, target_position: Vec3, angle: Vec3, sub_type: i32, target: Option<Target>, scale: f32) {
        self.spawns.push(Spawn::Joint(Joint::new(ty, position, target_position, angle, sub_type, target, scale)));
    }

    /// `CreateJoint` con `PKKey` y `SkillIndex` (los que el original pasa a veces).
    #[allow(clippy::too_many_arguments)]
    pub fn create_joint_ex(&mut self, ty: i32, position: Vec3, target_position: Vec3, angle: Vec3, sub_type: i32, target: Option<Target>, scale: f32, pk_key: i32, skill: i32) {
        let mut j = Joint::new(ty, position, target_position, angle, sub_type, target, scale);
        j.pk_key = pk_key;
        j.skill = skill;
        self.spawns.push(Spawn::Joint(j));
    }

    /// `CreateEffect(Type, Position, Angle, Light, SubType, Owner, PKKey, Skill, SkillIndex, ..., Scale)`.
    /// Devuelve el número del efecto creado.
    #[allow(clippy::too_many_arguments)]
    pub fn create_effect(&mut self, ty: i32, position: Vec3, angle: Vec3, light: Vec3, sub_type: i32, owner: Option<Target>, pk_key: i32, kind: i32, skill: i32, scale: f32) -> u32 {
        *self.next_effect_id += 1;
        let id = *self.next_effect_id;
        self.spawns.push(Spawn::Effect(Effect::new(id, ty, position, angle, light, sub_type, owner, pk_key, kind, skill, scale)));
        id
    }

    /// `CreateSprite(Type, Position, Scale, Light, Owner, Rotation, SubType)`: dura este tick.
    pub fn create_sprite(&mut self, ty: i32, position: Vec3, scale: f32, light: Vec3, rotation: f32, sub_type: i32) {
        self.sprites.push(SpriteFx { ty, position, scale, light, rotation, sub_type, alpha: 1.0 });
    }
}

// --- Dibujo ----------------------------------------------------------------------------------------

/// Un sprite a dibujar este fotograma (`RenderSprite`): centrado en `position`, `width`×`height`
/// unidades de MU, girado `rotation` grados en el plano de la pantalla.
#[derive(Clone, Copy)]
pub struct SpriteDraw {
    pub texture: i32,
    pub position: Vec3,
    pub width: f32,
    pub height: f32,
    /// Color (rgb) y alfa (a), como el color del vértice del original.
    pub color: [f32; 4],
    pub rotation: f32,
    /// (u, v, ancho, alto) en la textura; v = 0 arriba.
    pub uv: [f32; 4],
    pub blend: Blend,
}

/// Marca en el suelo (`RenderTerrainAlphaBitmap`): un cuadrado pegado al terreno.
#[derive(Clone, Copy)]
pub struct DecalDraw {
    pub texture: i32,
    /// Centro en MU (x, y).
    pub center: Vec2,
    /// Tamaño en unidades de MU.
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
    pub rotation: f32,
    /// Altura sobre el suelo (unidades de MU).
    pub lift: f32,
    pub blend: Blend,
}

/// Un modelo `.bmd` a dibujar este fotograma (`RenderObject` de un efecto).
#[derive(Clone, Copy)]
pub struct ModelDraw {
    /// `MODEL_*` (efectos, ítems...).
    pub model: i32,
    pub position: Vec3,
    pub angle: Vec3,
    pub scale: f32,
    /// `BodyLight`: color de todo el modelo.
    pub light: Vec3,
    pub alpha: f32,
    pub action: usize,
    pub frame: f32,
    /// Malla (por su índice de textura) que se suma con `BlendMeshLight` y desplazamiento de UV.
    pub blend_mesh: i32,
    pub blend_mesh_light: f32,
    pub blend_mesh_tex_coord: Vec2,
    /// `HiddenMesh`: -1 ninguna, -2 todas, n = esa.
    pub hidden_mesh: i32,
    /// `RENDER_BRIGHT`: todo el modelo sumando.
    pub bright: bool,
}

impl ModelDraw {
    pub fn new(model: i32, position: Vec3, angle: Vec3, scale: f32) -> Self {
        Self {
            model,
            position,
            angle,
            scale,
            light: Vec3::ONE,
            alpha: 1.0,
            action: 0,
            frame: 0.0,
            blend_mesh: -1,
            blend_mesh_light: 1.0,
            blend_mesh_tex_coord: Vec2::ZERO,
            hidden_mesh: -1,
            bright: false,
        }
    }

    /// El modelo de un efecto con su estado (lo que hace `RenderObject`).
    pub fn of_effect(e: &Effect) -> Self {
        Self {
            model: e.ty,
            position: e.position,
            angle: e.angle,
            scale: e.scale,
            light: e.light,
            alpha: e.alpha,
            action: e.current_action,
            frame: e.animation_frame,
            blend_mesh: e.blend_mesh,
            blend_mesh_light: e.blend_mesh_light,
            blend_mesh_tex_coord: e.blend_mesh_tex_coord,
            hidden_mesh: e.hidden_mesh,
            bright: false,
        }
    }
}

/// Un cuadrado libre en el mundo (`RenderPlane3D`, cintas hechas a mano...): 4 esquinas en MU.
#[derive(Clone, Copy)]
pub struct QuadDraw {
    pub texture: i32,
    pub corners: [Vec3; 4],
    /// (u, v) de cada esquina.
    pub uv: [[f32; 2]; 4],
    pub color: [f32; 4],
    pub blend: Blend,
}

/// Lo que se dibuja este fotograma; lo rellenan las funciones `draw` de cada tipo.
#[derive(Default)]
pub struct DrawList {
    pub sprites: Vec<SpriteDraw>,
    pub decals: Vec<DecalDraw>,
    pub quads: Vec<QuadDraw>,
    pub models: Vec<ModelDraw>,
}

/// Tamaño en píxeles y canales (3 = JPG, 4 = TGA) de una textura `BITMAP_*`.
#[derive(Clone, Copy, Debug)]
pub struct TexInfo {
    pub width: f32,
    pub height: f32,
    pub components: u8,
}

/// Lo que ve una función de dibujo: consultar texturas y el tiempo.
pub struct DrawCtx<'a> {
    pub world_time: f32,
    pub tex_info: &'a dyn Fn(i32) -> Option<TexInfo>,
    pub actors: &'a std::collections::HashMap<Entity, Actor>,
}

impl DrawCtx<'_> {
    pub fn tex(&self, id: i32) -> Option<TexInfo> {
        (self.tex_info)(id)
    }
}

// --- Registro de tipos -----------------------------------------------------------------------------

pub type ParticleCreate = fn(&mut Particle, &mut Ctx);
pub type ParticleStep = fn(&mut Particle, &mut Ctx);
pub type ParticleDraw = fn(&Particle, &DrawCtx, &mut DrawList);
pub type JointCreate = fn(&mut Joint, &mut Ctx);
pub type JointStep = fn(&mut Joint, &mut Ctx);
pub type EffectCreate = fn(&mut Effect, &mut Ctx);
pub type EffectStep = fn(&mut Effect, &mut Ctx);
pub type EffectDraw = fn(&Effect, &DrawCtx, &mut DrawList);

/// Funciones de un tipo de partícula (lo que el original tiene en los `switch` de
/// `CreateParticle`, `MoveParticles` y `DescribeParticle`). `None` = comportamiento por defecto.
#[derive(Clone, Copy, Default)]
pub struct ParticleKind {
    pub create: Option<ParticleCreate>,
    pub step: Option<ParticleStep>,
    pub draw: Option<ParticleDraw>,
}

#[derive(Clone, Copy, Default)]
pub struct JointKind {
    pub create: Option<JointCreate>,
    pub step: Option<JointStep>,
}

#[derive(Clone, Copy, Default)]
pub struct EffectKind {
    pub create: Option<EffectCreate>,
    pub step: Option<EffectStep>,
    /// Si es `None` se dibuja el modelo del tipo (si existe) con [`ModelDraw::of_effect`].
    pub draw: Option<EffectDraw>,
}

/// Todos los tipos registrados (cada módulo de `fx/kinds/` añade los suyos).
#[derive(Default, bevy::prelude::Resource)]
pub struct Registry {
    pub particles: std::collections::HashMap<i32, ParticleKind>,
    pub joints: std::collections::HashMap<i32, JointKind>,
    pub effects: std::collections::HashMap<i32, EffectKind>,
    /// Habilidades por número `AT_SKILL_*`.
    pub skills: std::collections::HashMap<i32, SkillKind>,
}

impl Registry {
    pub fn skill(&mut self, skill: i32, kind: SkillKind) {
        self.skills.insert(skill, kind);
    }
    pub fn particle(&mut self, ty: i32, kind: ParticleKind) {
        self.particles.insert(ty, kind);
    }
    pub fn joint(&mut self, ty: i32, kind: JointKind) {
        self.joints.insert(ty, kind);
    }
    pub fn effect(&mut self, ty: i32, kind: EffectKind) {
        self.effects.insert(ty, kind);
    }
}

/// Dibujo por defecto de una partícula (`RenderParticles`): un sprite del tamaño de su textura
/// por la escala, con la mezcla según la textura (JPG = suma, TGA = prueba de alfa).
pub fn default_particle_draw(p: &Particle, ctx: &DrawCtx, out: &mut DrawList) {
    let Some(t) = ctx.tex(p.tex_type) else { return };
    out.sprites.push(SpriteDraw {
        texture: p.tex_type,
        position: p.position,
        width: t.width * p.scale,
        height: t.height * p.scale,
        color: [p.light.x, p.light.y, p.light.z, if t.components == 3 { 1.0 } else { p.light.x }],
        rotation: p.rotation,
        uv: [0.0, 0.0, 1.0, 1.0],
        blend: default_blend(t),
    });
}

/// Mezcla por defecto de partículas y marcas: JPG (3 canales) suma; TGA prueba de alfa.
pub fn default_blend(t: TexInfo) -> Blend {
    if t.components == 3 { Blend::Glow } else { Blend::AlphaTest }
}
