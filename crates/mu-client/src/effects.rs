//! Partículas del mapa: fuego y humo (`CreateFire`, `CreateParticle` y `MoveParticles` del original).
//!
//! **Independiente de los fps:** el original hace todo "por tick" (25 por segundo): la gravedad
//! suma 0.004, la velocidad se multiplica por 0.98, aparece una llama 1 de cada 2 veces...
//! Aquí la simulación corre en `FixedUpdate` a exactamente 25 Hz con esas mismas fórmulas, y al
//! dibujar se interpola entre el tick anterior y el actual. A 60 o 144 fps se ve fluido y va a
//! la misma velocidad que el original.
//!
//! Todo se simula en coordenadas de MU (Z arriba, 100 por casilla) y se pasa a Bevy al dibujar.
//! Cada fotograma se rehace una malla por tipo de sprite con un cuadrado mirando a la cámara.

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::objects::{EmitKind, Emitter, TICKS_PER_SECOND};
use mu_data::texture::{self, Texture};

use crate::Args;
use crate::terrain::{MU_SCALE, TerrainLights, to_bevy_image};

/// Distancia (MU) hasta la que los fuegos iluminan el suelo.
const LIGHT_DISTANCE: f32 = 3000.0;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(TICKS_PER_SECOND as f64))
            .init_resource::<Emitters>()
            .init_resource::<FireRequests>()
            .init_resource::<Particles>()
            .add_systems(Startup, setup_sprites)
            .add_systems(FixedUpdate, (emit, step).chain())
            .add_systems(PostUpdate, draw.after(crate::camera::CameraMoved).before(TransformSystems::Propagate));
    }
}

/// Un emisor colocado en el mapa (lo registra `objects.rs` al cargar los objetos).
pub struct PlacedEmitter {
    pub emitter: Emitter,
    /// Posición, giro y escala del objeto (en MU).
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    /// Ángulo X del objeto en grados: el original gira con él el sprite de las llamas.
    pub angle_x: f32,
    /// Para los emisores pegados a un hueso: el modelo y su velocidad de animación.
    pub model: Option<(Arc<Bmd>, f32)>,
}

#[derive(Resource, Default)]
pub struct Emitters(pub Vec<PlacedEmitter>);

/// Llamas sueltas que piden otros sistemas para este tick (p. ej. la cabeza de los dragones):
/// (posición en MU, luz). Se crean en el siguiente `emit`.
#[derive(Resource, Default)]
pub struct FireRequests(pub Vec<(Vec3, Vec3)>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `BITMAP_FIRE` con su subtipo (0..=3).
    Fire(u8),
    /// `BITMAP_SMOKE` subtipo 0.
    Smoke,
    /// `BITMAP_SMOKE` subtipo 2 (se dibuja oscureciendo).
    DarkSmoke,
}

struct Particle {
    kind: Kind,
    pos: Vec3,
    prev_pos: Vec3,
    vel: Vec3,
    /// Giro con el que se aplica la velocidad (`MovePosition` usa el ángulo del objeto).
    rot: Quat,
    life: i32,
    gravity: f32,
    scale: f32,
    prev_scale: f32,
    light: Vec3,
    prev_light: Vec3,
    /// Giro del sprite en pantalla, en grados.
    rotation: f32,
    frame: i32,
}

#[derive(Resource)]
struct Particles {
    list: Vec<Particle>,
    rng: u32,
    ticks: u64,
}

impl Default for Particles {
    fn default() -> Self {
        Self { list: vec![], rng: 0x2545_F491, ticks: 0 }
    }
}

impl Particles {
    /// `rand()` del original (entero no negativo).
    fn rand(&mut self) -> u32 {
        let s = &mut self.rng;
        *s ^= *s << 13;
        *s ^= *s >> 17;
        *s ^= *s << 5;
        *s >> 1
    }
    /// `Random::RangeFloat(a, b)`: decimal entre a y b.
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * (self.rand() % 10_000) as f32 / 9_999.0
    }
    /// `rand_fps_check(n)`: 1 de cada n ticks.
    fn chance(&mut self, n: u32) -> bool {
        self.rand() % n == 0
    }
}

/// Un tipo de sprite: textura, malla que se rehace cada fotograma y su tamaño en píxeles.
struct Sprite {
    mesh: Handle<Mesh>,
    size: Vec2,
}

#[derive(Resource)]
struct Sprites {
    fire: Option<Sprite>,
    smoke: Option<Sprite>,
    dark_smoke: Option<Sprite>,
}

fn setup_sprites(
    mut commands: Commands,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let dir = args.data.join("Effect");
    let mut make = |tex: Texture, alpha_mode: AlphaMode, name: &str| {
        let size = Vec2::new(tex.width as f32, tex.height as f32);
        let mesh = meshes.add(empty_mesh());
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(images.add(to_bevy_image(&tex))),
            unlit: true,
            cull_mode: None,
            alpha_mode,
            ..default()
        });
        commands.spawn((Name::new(format!("Partículas {name}")), Mesh3d(mesh.clone()), MeshMaterial3d(material)));
        Sprite { mesh, size }
    };
    let load = |name: &str| match texture::load(&dir, name) {
        Ok(t) => Some(t),
        Err(e) => {
            warn!("sin textura de efecto {name}: {e}");
            None
        }
    };

    // Texturas sin alfa (JPG): el original las suma a lo que hay detrás.
    let fire = load("Fire01.OZJ").map(|t| make(t, AlphaMode::Add, "fuego"));
    let smoke_tex = load("smoke01.OZJ");
    let smoke = smoke_tex.clone().map(|t| make(t, AlphaMode::Add, "humo"));
    // Humo oscuro: el original lo dibuja restando (`dst · (1 - src)`). Equivale a pintar negro
    // con la transparencia que marca el brillo de la textura.
    let dark_smoke = smoke_tex.map(|mut t| {
        for px in t.rgba.chunks_exact_mut(4) {
            let lum = (px[0] as u32 + px[1] as u32 + px[2] as u32) / 3;
            px.copy_from_slice(&[0, 0, 0, lum as u8]);
        }
        make(t, AlphaMode::Blend, "humo oscuro")
    });
    commands.insert_resource(Sprites { fire, smoke, dark_smoke });
}

fn empty_mesh() -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new())
        .with_inserted_indices(Indices::U32(vec![]))
}

/// Un tick de los emisores (`CreateFire` y las salpicaduras de la fuente).
fn emit(
    emitters: Res<Emitters>,
    mut ps: ResMut<Particles>,
    mut lights: ResMut<TerrainLights>,
    players: Query<&crate::character::Player>,
    time: Res<Time<Fixed>>,
    mut requests: ResMut<FireRequests>,
) {
    let world_ms = (time.elapsed_secs() * 1000.0) as u32;
    // Llamas pedidas por otros (`CreateParticle(BITMAP_FIRE, ..., subtipo 0)`).
    for (pos, light) in requests.0.drain(..) {
        let vel = -((ps.rand() % 16 + 32) as f32) * 0.1;
        let scale = (ps.rand() % 64 + 128) as f32 * 0.01;
        let p = new_particle(Kind::Fire(0), pos, Vec3::new(0.0, vel, 0.0), Quat::IDENTITY, 24, scale, light, 0.0);
        ps.list.push(p);
    }
    // Las luces en el suelo solo importan cerca del personaje (lo que se ve).
    let hero = players.single().map(|p| p.pos * MU_SCALE).ok();
    for e in &emitters.0 {
        let em = &e.emitter;
        if em.kind == EmitKind::Fire
            && let Some(h) = hero
            && (e.position.truncate() - h).length() < LIGHT_DISTANCE
        {
            // `CreateFire(0)`: luz naranja de radio 4 en cada tick, aunque no salga llama.
            let l = ps.range(6.0, 11.0) * 0.1;
            let p = e.position + e.rotation * Vec3::from_array(em.offset);
            lights.add(p.truncate() / MU_SCALE, [l, 0.6 * l, 0.4 * l], 4.0);
        }
        // Posición del emisor en el mundo (MU).
        let local = match (em.bone, &e.model) {
            (Some(bone), Some((model, fps))) => {
                let spread = em.spread;
                let off = Vec3::new(
                    em.offset[0] + ps.range(-spread, spread - 1.0),
                    em.offset[1],
                    em.offset[2] + ps.range(-spread, spread - 1.0),
                );
                let bones = model.bone_matrices_at(0, time.elapsed_secs() * fps);
                let Some(m) = bones.get(bone) else { continue };
                Vec3::from_array(bmd::transform_point(m, off.to_array())) * e.scale
            }
            _ => Vec3::from_array(em.offset),
        };
        let mut pos = e.position + e.rotation * local;
        if em.bone.is_none() {
            // `CreateFire` mueve un poco cada llama al azar.
            pos += Vec3::new(ps.range(-8.0, 7.0), ps.range(-8.0, 7.0), ps.range(-8.0, 7.0));
        }
        if !ps.chance(2) {
            continue;
        }
        let p = match em.kind {
            EmitKind::Fire => {
                let l = ps.range(6.0, 11.0) * 0.1;
                let sub = (ps.rand() % 4) as u8;
                let (vel, scale) = match sub {
                    0 => (-((ps.rand() % 16 + 32) as f32) * 0.1, (ps.rand() % 64 + 128) as f32 * 0.01),
                    1 => (-((ps.rand() % 16 + 32) as f32) * 0.1, (ps.rand() % 4 + 10) as f32 * 0.01),
                    _ => (-((ps.rand() % 32) as f32 - 16.0) * 0.1, 1.0),
                };
                new_particle(Kind::Fire(sub), pos, Vec3::new(0.0, vel, 0.0), e.rotation, 24, scale, Vec3::new(l, 0.6 * l, 0.4 * l), e.angle_x)
            }
            EmitKind::Smoke => {
                let scale = (ps.rand() % 32 + 48) as f32 * 0.01;
                let mut p = new_particle(Kind::Smoke, pos, Vec3::ZERO, e.rotation, 16, scale, Vec3::splat(2.0), 0.0);
                p.rotation = (world_ms % 360) as f32;
                p
            }
            EmitKind::DarkSmoke => {
                let scale = (ps.rand() % 64 + 64) as f32 * 0.01;
                let rotation = (ps.rand() % 360) as f32;
                let mut p = new_particle(Kind::DarkSmoke, pos, Vec3::ZERO, e.rotation, 50, scale, Vec3::ONE, rotation);
                p.gravity = (ps.rand() % 32 + 60) as f32 * 0.1;
                p
            }
        };
        ps.list.push(p);
    }
}

#[allow(clippy::too_many_arguments)]
fn new_particle(kind: Kind, pos: Vec3, vel: Vec3, rot: Quat, life: i32, scale: f32, light: Vec3, rotation: f32) -> Particle {
    Particle {
        kind,
        pos,
        prev_pos: pos,
        vel,
        rot,
        life,
        gravity: 0.0,
        scale,
        prev_scale: scale,
        light,
        prev_light: light,
        rotation,
        frame: 0,
    }
}

/// Un tick de `MoveParticles` para los tipos que usamos.
fn step(mut ps: ResMut<Particles>) {
    ps.ticks += 1;
    ps.list.retain_mut(|p| {
        p.prev_pos = p.pos;
        p.prev_scale = p.scale;
        p.prev_light = p.light;
        p.life -= 1;
        if p.life <= 0 {
            return false;
        }
        p.pos += p.rot * p.vel;
        match p.kind {
            Kind::Fire(0) => {
                p.gravity += 0.004;
                p.scale -= 0.04;
                p.frame = (23 - p.life) / 6;
                p.pos.z += p.gravity * 10.0;
            }
            Kind::Fire(_) => {
                p.gravity += 0.004;
                p.scale += p.gravity;
                p.vel *= 0.98;
                p.frame = (23 - p.life) / 6;
                p.pos.z += p.gravity * 10.0;
            }
            Kind::Smoke => {
                p.light = Vec3::splat(p.life as f32 / 8.0);
                p.gravity += 0.2;
                p.pos.z += p.gravity;
                p.scale += 0.05;
            }
            Kind::DarkSmoke => {
                p.light = Vec3::splat(p.life as f32 / 50.0);
                p.gravity -= 0.1;
                p.pos.x -= p.gravity * 0.2;
                p.pos.z += p.gravity;
                p.scale -= 0.01;
            }
        }
        p.scale > 0.0
    });
}

/// Rehace las mallas de sprites mirando a la cámara, interpolando entre los dos últimos ticks.
fn draw(
    ps: Res<Particles>,
    sprites: Option<Res<Sprites>>,
    fixed: Res<Time<Fixed>>,
    cameras: Query<&Transform, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(sprites), Ok(cam)) = (sprites, cameras.single()) else { return };
    let a = fixed.overstep_fraction();
    let (right, up) = (cam.right().as_vec3(), cam.up().as_vec3());

    let mut batches: [(Option<&Sprite>, Batch); 3] = [
        (sprites.fire.as_ref(), Batch::default()),
        (sprites.smoke.as_ref(), Batch::default()),
        (sprites.dark_smoke.as_ref(), Batch::default()),
    ];
    for p in &ps.list {
        let slot = match p.kind {
            Kind::Fire(_) => 0,
            Kind::Smoke => 1,
            Kind::DarkSmoke => 2,
        };
        let Some(sprite) = batches[slot].0 else { continue };
        let pos = p.prev_pos.lerp(p.pos, a);
        let scale = p.prev_scale + (p.scale - p.prev_scale) * a;
        let light = p.prev_light.lerp(p.light, a);
        // MU (x, y, z) → Bevy (x, z, -y).
        let center = Vec3::new(pos.x, pos.z, -pos.y) / MU_SCALE;
        let (size, uv, color) = match p.kind {
            // Tira de 4 fotogramas en horizontal.
            Kind::Fire(_) => {
                let u = (p.frame.rem_euclid(4)) as f32 * 0.25;
                (Vec2::new(sprite.size.x * 0.25, sprite.size.y) * scale, [u, 0.0, u + 0.25, 1.0], light)
            }
            Kind::Smoke => (sprite.size * scale, [0.0, 0.0, 1.0, 1.0], light),
            Kind::DarkSmoke => (sprite.size * scale, [0.0, 0.0, 1.0, 1.0], light),
        };
        let rotation = p.rotation;
        // La luz del original multiplica en sRGB; en lineal es elevarla a 2.2.
        let lin = color.max(Vec3::ZERO).powf(2.2);
        let rgba = if p.kind == Kind::DarkSmoke { [0.0, 0.0, 0.0, light.x.clamp(0.0, 1.0)] } else { [lin.x, lin.y, lin.z, 1.0] };
        batches[slot].1.quad(center, size / MU_SCALE, rotation, right, up, uv, rgba);
    }

    for (sprite, batch) in batches {
        let Some(sprite) = sprite else { continue };
        if let Some(mut m) = meshes.get_mut(&sprite.mesh) {
            let n = batch.pos.len();
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, batch.pos);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, batch.uv);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, batch.color);
            m.insert_indices(Indices::U32(batch.idx));
        }
    }
}

#[derive(Default)]
struct Batch {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    color: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Batch {
    /// Cuadrado como `RenderSprite`: centrado, girado `rotation` grados en el plano de la pantalla.
    /// `uv` = [u0, v0, u1, v1] con v0 arriba.
    #[allow(clippy::too_many_arguments)]
    fn quad(&mut self, center: Vec3, size: Vec2, rotation: f32, right: Vec3, up: Vec3, uv: [f32; 4], rgba: [f32; 4]) {
        let (s, c) = rotation.to_radians().sin_cos();
        let (w, h) = (size.x * 0.5, size.y * 0.5);
        let base = self.pos.len() as u32;
        // Abajo-izquierda, abajo-derecha, arriba-derecha, arriba-izquierda (igual que el original).
        let corners = [(-w, -h, uv[0], uv[3]), (w, -h, uv[2], uv[3]), (w, h, uv[2], uv[1]), (-w, h, uv[0], uv[1])];
        for (dx, dy, u, v) in corners {
            let x = dx * c - dy * s;
            let y = dx * s + dy * c;
            self.pos.push((center + right * x + up * y).to_array());
            self.uv.push([u, v]);
            self.color.push(rgba);
        }
        self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}
