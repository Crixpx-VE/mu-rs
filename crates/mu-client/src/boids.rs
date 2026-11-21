//! Pájaros, murciélagos y mariposas que vuelan alrededor del personaje, y los dragones de la
//! invasión (`MoveBoids`, `MoveBird`, `MoveBoidGroup` y `MoveBoid` de GOBoid.cpp / ZzzAI.cpp).
//!
//! - 5 a la vez, cerca del personaje: pájaros en Lorencia (`Object1/Bird01`), murciélagos en
//!   Dungeon y Lost Tower (`Object2/Bat01`), mariposas en Noria (`Object1/Butterfly01`).
//! - Vuelan en bandada: cada uno gira hasta 13° por tick hacia la media de los vecinos (a menos
//!   de 400) y se aparta de los que tiene a menos de 80.
//! - Los pájaros bajan a posarse cerca del personaje (a 200-400) unos 2 de cada 8 segundos si
//!   está quieto, y vuelven a volar cuando se mueve. Se van (y nace otro) 1 de cada 512 ticks o
//!   si quedan a más de 1500.
//! - Durante la invasión (ver `invasion.rs`) los huecos se llenan de dragones
//!   (`Monster/Monster32.bmd`) que cruzan hacia -Y con fuego en la cabeza.
//!
//! Como las partículas: la lógica corre a 25 ticks por segundo y el dibujo interpola.

use std::f32::consts::FRAC_PI_2;
use std::path::Path;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::terrain::{SIZE, attr};
use mu_data::{fs, texture};

use crate::Args;
use crate::character::Player;
use crate::effects::FireRequests;
use crate::invasion::Invasion;
use crate::terrain::{LoadedTerrain, MU_SCALE, to_bevy_image};

pub struct BoidsPlugin;

impl Plugin for BoidsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, setup_boids)
            .add_systems(FixedUpdate, step_boids)
            .add_systems(Update, draw_boids);
    }
}

const MAX_BOIDS: usize = 5;
const NEIGHBOR_RANGE: f32 = 400.0;
const SEPARATION_RANGE: f32 = 80.0;
/// Acción de los dragones de la invasión (`MONSTER01_DIE + 1`, "aparecer") y su velocidad.
const DRAGON_ACTION: usize = 7;
const DRAGON_PLAY_SPEED: f32 = 0.5;
/// Hueso de la cabeza del dragón, donde el original pone el fuego.
const DRAGON_HEAD_BONE: usize = 11;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bird,
    Bat,
    Butterfly,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ai {
    Fly,
    Down,
    Ground,
    Up,
}

/// Acciones del pájaro.
const BIRD_FLY: usize = 0;
const BIRD_GROUND: usize = 1;

/// Un modelo dibujable para un hueco: entidad con una malla por cada malla del `.bmd`.
struct Body {
    entity: Entity,
    parts: Vec<(usize, Handle<Mesh>)>,
    materials: Vec<Handle<StandardMaterial>>,
}

struct Boid {
    live: bool,
    dragon: bool,
    golden: bool,
    pos: Vec3,
    prev_pos: Vec3,
    /// Rumbo en grados alrededor del eje Z de MU.
    angle: f32,
    prev_angle: f32,
    /// Velocidad vertical (`Direction[2]`) y punto al que va (`Direction[0..2]`).
    dir_z: f32,
    look: Vec2,
    velocity: f32,
    ai: Ai,
    timer: f32,
    action: usize,
    frame: f32,
    scale: f32,
    life: i32,
    bird: Option<Body>,
    dragon_body: Option<Body>,
    /// Su sombra en el suelo (`RenderBodyShadow`), del modelo que esté activo.
    shadow: crate::shadow::Shadow,
}

#[derive(Resource)]
struct Boids {
    kind: Option<Kind>,
    model: Option<Arc<Bmd>>,
    dragon: Option<Arc<Bmd>>,
    list: Vec<Boid>,
    rng: u32,
    event: Invasion,
}

impl Boids {
    fn rand(&mut self) -> i32 {
        let s = &mut self.rng;
        *s ^= *s << 13;
        *s ^= *s >> 17;
        *s ^= *s << 5;
        (*s >> 1) as i32
    }
    fn chance(&mut self, n: i32) -> bool {
        self.rand() % n == 0
    }
}

fn load_bmd(dir: &Path, file: &str) -> Option<Arc<Bmd>> {
    match fs::read(dir, file).and_then(|b| bmd::read(&b)) {
        Ok(m) => Some(Arc::new(m)),
        Err(e) => {
            warn!("sin {file}: {e}");
            None
        }
    }
}

/// Crea la entidad (oculta) de un modelo con sus mallas para animar.
fn spawn_body(
    commands: &mut Commands,
    name: String,
    model: &Bmd,
    textures: &[Option<Handle<Image>>],
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Body {
    let mut parts = vec![];
    let mut mats = vec![];
    let entity = commands
        .spawn((Name::new(name), Transform::default(), Visibility::Hidden))
        .with_children(|p| {
            for (mi, tex) in textures.iter().enumerate() {
                let posed = model.pose(mi, 0, 0);
                if posed.positions.is_empty() {
                    continue;
                }
                let mesh = meshes.add(
                    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs),
                );
                let mat = materials.add(StandardMaterial {
                    base_color_texture: tex.clone(),
                    unlit: true,
                    cull_mode: None,
                    alpha_mode: AlphaMode::Mask(crate::objects::ALPHA_REF),
                    ..default()
                });
                p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone())));
                parts.push((mi, mesh));
                mats.push(mat);
            }
        })
        .id();
    Body { entity, parts, materials: mats }
}

fn textures_of(model: &Bmd, dir: &Path, images: &mut Assets<Image>) -> Vec<Option<Handle<Image>>> {
    model
        .meshes
        .iter()
        .map(|m| texture::load(dir, &bmd::texture_file_name(&m.texture)).ok().map(|t| images.add(to_bevy_image(&t))))
        .collect()
}

fn setup_boids(
    mut commands: Commands,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
) {
    let (kind, dir, file) = match args.world {
        1 => (Some(Kind::Bird), "Object1", "Bird01.bmd"),
        2 | 5 => (Some(Kind::Bat), "Object2", "Bat01.bmd"),
        4 => (Some(Kind::Butterfly), "Object1", "Butterfly01.bmd"),
        _ => (None, "", ""),
    };
    let bird_dir = args.data.join(dir);
    let model = kind.and_then(|_| load_bmd(&bird_dir, file));
    let bird_tex = model.as_ref().map(|m| textures_of(m, &bird_dir, &mut images));
    let dragon_dir = args.data.join("Monster");
    let dragon = load_bmd(&dragon_dir, "Monster32.bmd");
    let dragon_tex = dragon.as_ref().map(|m| textures_of(m, &dragon_dir, &mut images));

    let mut list = Vec::with_capacity(MAX_BOIDS);
    for i in 0..MAX_BOIDS {
        let bird = model.as_ref().zip(bird_tex.as_ref()).map(|(m, t)| {
            spawn_body(&mut commands, format!("Bicho {i}"), m, t, &mut meshes, &mut materials)
        });
        let dragon_body = dragon.as_ref().zip(dragon_tex.as_ref()).map(|(m, t)| {
            spawn_body(&mut commands, format!("Dragón {i}"), m, t, &mut meshes, &mut materials)
        });
        list.push(Boid {
            live: false,
            dragon: false,
            golden: false,
            pos: Vec3::ZERO,
            prev_pos: Vec3::ZERO,
            angle: 0.0,
            prev_angle: 0.0,
            dir_z: 0.0,
            look: Vec2::ZERO,
            velocity: 1.0,
            ai: Ai::Fly,
            timer: 0.0,
            action: 0,
            frame: 0.0,
            scale: 0.8,
            life: 0,
            bird,
            dragon_body,
            shadow: crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material),
        });
    }
    commands.insert_resource(Boids {
        kind: kind.filter(|_| model.is_some()),
        model,
        dragon,
        list,
        rng: 0x51A3_77C1,
        event: Invasion::default(),
    });
}

/// Rumbo de MU (`CreateAngle`): 0 = hacia -Y, en grados.
fn create_angle(from: Vec2, to: Vec2) -> f32 {
    let d = to - from;
    if d.length_squared() < 1e-12 {
        return 0.0;
    }
    d.x.atan2(-d.y).to_degrees().rem_euclid(360.0)
}

/// `TurnAngle2`: gira `angle` hacia `target` como mucho `step` grados.
fn turn_angle(angle: f32, target: f32, step: f32) -> f32 {
    let delta = (target - angle + 540.0).rem_euclid(360.0) - 180.0;
    (angle + delta.clamp(-step, step)).rem_euclid(360.0)
}

fn frames(model: Option<&Arc<Bmd>>, action: usize) -> f32 {
    model.and_then(|m| m.actions.get(action)).map_or(1, |a| a.frames.max(1)) as f32
}

#[allow(clippy::too_many_arguments)]
fn step_boids(
    boids: Option<ResMut<Boids>>,
    terrain: Option<Res<LoadedTerrain>>,
    players: Query<&Player>,
    time: Res<Time<Fixed>>,
    invasion: Res<Invasion>,
    mut fire: ResMut<FireRequests>,
) {
    let (Some(mut b), Some(t), Ok(hero)) = (boids, terrain, players.single()) else { return };
    let t = &t.0;
    let world_ms = (time.elapsed_secs() * 1000.0) as u32;
    let hero_mu = Vec3::new(hero.pos.x * MU_SCALE, hero.pos.y * MU_SCALE, t.height_at(hero.pos.x, hero.pos.y));
    let hero_still = hero.path.is_empty();
    let ground = |p: Vec3| t.height_at(p.x / MU_SCALE, p.y / MU_SCALE);
    let kind = b.kind;

    // Al empezar o acabar el evento, el original borra todos los bichos (`DeleteBoids`).
    if b.event != *invasion {
        b.event = *invasion;
        for o in &mut b.list {
            o.live = false;
        }
    }

    for i in 0..MAX_BOIDS {
        if !b.list[i].live {
            if invasion.active() {
                // Un dragón 1 de cada 300 ticks por hueco libre.
                if b.dragon.is_some() && b.chance(300) {
                    spawn_dragon(&mut b, i, hero_mu, invasion.0 == 3);
                }
            } else if kind.is_some() {
                spawn_boid(&mut b, i, hero_mu, kind == Some(Kind::Butterfly), &ground);
            }
            if !b.list[i].live {
                continue;
            }
        }
        {
            let o = &mut b.list[i];
            o.prev_pos = o.pos;
            o.prev_angle = o.angle;
        }

        if b.list[i].dragon {
            step_dragon(&mut b, i, &ground, &mut fire);
            continue;
        }
        let Some(kind) = kind else { continue };

        // Animación: 1 fotograma por tick.
        let n = frames(b.model.as_ref(), b.list[i].action);
        b.list[i].frame = (b.list[i].frame + 1.0) % n;

        // Comportamiento propio.
        match kind {
            Kind::Bird => {
                let r = [b.rand() % 16 - 8, b.rand() % 256, b.rand() % 16 - 8];
                let o = &mut b.list[i];
                if o.ai == Ai::Fly {
                    let dist = (o.pos.truncate() - hero_mu.truncate()).length();
                    // Solo bajan a posarse si el personaje está quieto: si no, rebotan en el
                    // suelo y vuelven a bajar una y otra vez a su lado (parece que lo persiguen).
                    if hero_still && world_ms % 8192 < 2048 && (200.0..=400.0).contains(&dist) {
                        o.ai = Ai::Down;
                    }
                    o.velocity = 1.0;
                    o.pos.z += r[0] as f32;
                    if o.pos.z < 200.0 {
                        o.dir_z = 10.0;
                    } else if o.pos.z > 600.0 {
                        o.dir_z = -10.0;
                    }
                }
                if o.ai == Ai::Down {
                    o.dir_z = -20.0;
                    let h = ground(o.pos);
                    if o.pos.z < h {
                        let (tx, ty) = ((o.pos.x / MU_SCALE) as usize, (o.pos.y / MU_SCALE) as usize);
                        let can_land = tx < SIZE && ty < SIZE && t.attribute(tx, ty) & attr::NO_MOVE == 0;
                        if can_land && hero_still {
                            o.pos.z = h;
                            o.ai = Ai::Ground;
                            o.action = BIRD_GROUND;
                        } else {
                            take_off(o);
                        }
                    }
                }
                if o.ai == Ai::Ground && (!hero_still || r[1] == 0) {
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
            Kind::Bat => {
                let o = &mut b.list[i];
                o.pos.z = ground(o.pos) + (-(o.timer.sin().abs()) * 150.0 + 350.0);
                o.timer += 0.2;
            }
            Kind::Butterfly => {
                let turn = b.chance(32);
                let (ra, rd, rd2, rz) = (b.rand() % 360, b.rand() % 15 - 7, b.rand() % 15 - 7, b.rand() % 15 - 7);
                let o = &mut b.list[i];
                if turn {
                    o.angle = ra as f32;
                    o.dir_z = rd as f32;
                }
                o.dir_z += rd2 as f32 * 0.2;
                let h = ground(o.pos);
                if o.pos.z < h + 50.0 {
                    o.dir_z = o.dir_z * 0.8 + 1.0;
                }
                if o.pos.z > h + 300.0 {
                    o.dir_z = o.dir_z * 0.8 - 1.0;
                }
                o.pos.z += rz as f32 * 0.3;
            }
        }

        // Vuelo en grupo (`MoveBoidGroup`).
        if b.list[i].ai != Ai::Ground {
            // Las mariposas solo corrigen el rumbo 1 de cada 4 ticks.
            let steer = kind != Kind::Butterfly || b.chance(4);
            if steer {
                flock(&mut b.list, i, 13.0);
            }
            let o = &mut b.list[i];
            let (s, c) = o.angle.to_radians().sin_cos();
            // `AngleMatrix(0, 0, ángulo)` · (velocidad·25, 0, dir_z).
            let speed = o.velocity * 25.0;
            let p = Vec3::new(speed * c, speed * s, o.dir_z);
            o.pos += p;
            o.look = o.pos.truncate() + 3.0 * p.truncate();
            if b.chance(512) {
                b.list[i].live = false;
            }
        }
        let o = &mut b.list[i];
        if (o.pos.truncate() - hero_mu.truncate()).length() >= 1500.0 {
            o.live = false;
        }
    }
}

fn spawn_boid(b: &mut Boids, i: usize, hero: Vec3, butterfly: bool, ground: &impl Fn(Vec3) -> f32) {
    let (rx, ry, rz, timer) = (b.rand() % 1024 - 512, b.rand() % 1024 - 512, b.rand() % 200 + 150, b.rand() % 314);
    let o = &mut b.list[i];
    o.live = true;
    o.dragon = false;
    o.pos = hero + Vec3::new(rx as f32, ry as f32, 0.0);
    o.pos.z = ground(o.pos) + rz as f32;
    o.prev_pos = o.pos;
    o.angle = 0.0;
    o.prev_angle = 0.0;
    o.velocity = if butterfly { 0.3 } else { 1.0 };
    o.ai = Ai::Fly;
    o.dir_z = 0.0;
    o.look = o.pos.truncate();
    o.timer = timer as f32 * 0.01;
    o.action = BIRD_FLY;
    o.frame = 0.0;
    o.scale = 0.8;
}

fn spawn_dragon(b: &mut Boids, i: usize, hero: Vec3, golden: bool) {
    let (scale, life, timer) = (b.rand() % 3 + 6, b.rand() % 128, b.rand() % 10);
    let (rx, ry) = (b.rand() % 600 - 100, b.rand() % 400 + 200);
    let o = &mut b.list[i];
    o.live = true;
    o.dragon = true;
    o.golden = golden;
    o.scale = scale as f32 * 0.1;
    o.life = 128 + life;
    o.timer = timer as f32 * 0.1;
    o.angle = -90.0;
    o.prev_angle = o.angle;
    o.pos = hero + Vec3::new(rx as f32, ry as f32, 300.0);
    o.prev_pos = o.pos;
    o.action = DRAGON_ACTION;
    o.frame = 0.0;
}

/// Dragón de la invasión: avanza `escala·40` por tick hacia donde mira, entre 300 y 400 sobre el
/// suelo, con fuego en la cabeza; vive 128-255 ticks.
fn step_dragon(b: &mut Boids, i: usize, ground: &impl Fn(Vec3) -> f32, fire: &mut FireRequests) {
    let n = frames(b.dragon.as_ref(), DRAGON_ACTION);
    let head = b.dragon.clone();
    let o = &mut b.list[i];
    o.frame = (o.frame + DRAGON_PLAY_SPEED) % n;
    let (s, c) = o.angle.to_radians().sin_cos();
    let step = o.scale * 40.0;
    o.pos += Vec3::new(step * c, step * s, 0.0);
    o.pos.z = ground(o.pos) + 300.0 - o.timer.sin().abs() * 100.0 + 100.0;
    o.timer += o.scale * 0.05;
    o.life -= 1;
    if o.life <= 0 {
        o.live = false;
        return;
    }
    // Fuego en la cabeza (hueso 11, desplazado 50 hacia -Y), en la pose que se dibuja.
    if let Some(model) = head {
        let bones = model.bone_matrices_at(DRAGON_ACTION, o.frame);
        if let Some(m) = bones.get(DRAGON_HEAD_BONE) {
            let local = Vec3::from_array(bmd::transform_point(m, [0.0, -50.0, 0.0]));
            let rot = Quat::from_rotation_z((o.angle + 90.0).to_radians());
            fire.0.push((o.pos + rot * (local * o.scale), Vec3::ONE));
        }
    }
}

fn take_off(o: &mut Boid) {
    o.ai = Ai::Up;
    o.velocity = 1.1;
    o.dir_z = 20.0;
    o.action = BIRD_FLY;
}

/// `MoveBoid`: rumbo hacia la media de las direcciones de los vecinos, apartándose de los cercanos.
fn flock(list: &mut [Boid], i: usize, turn: f32) {
    let me = list[i].pos;
    let mut target = Vec2::ZERO;
    let mut count = 0;
    for (j, t) in list.iter().enumerate() {
        if j == i || !t.live || t.dragon {
            continue;
        }
        let d2 = (me - t.pos).length_squared();
        if d2 >= NEIGHBOR_RANGE * NEIGHBOR_RANGE {
            continue;
        }
        let mut dir = t.look - t.pos.truncate();
        if d2 < SEPARATION_RANGE * SEPARATION_RANGE {
            dir -= t.look - me.truncate();
        } else {
            dir += t.look - me.truncate();
        }
        if dir.length_squared() > 0.0 && dir.is_finite() {
            target += dir.normalize();
            count += 1;
        }
    }
    if count == 0 || target == Vec2::ZERO {
        return;
    }
    let goal = me.truncate() + target / count as f32;
    let heading = create_angle(me.truncate(), goal).round();
    list[i].angle = turn_angle(list[i].angle, heading, turn);
}

/// Coloca, gira y anima cada bicho interpolando entre los dos últimos ticks.
fn draw_boids(
    boids: Option<Res<Boids>>,
    fixed: Res<Time<Fixed>>,
    terrain: Option<Res<LoadedTerrain>>,
    mut q: Query<(&mut Transform, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(b) = boids else { return };
    let a = fixed.overstep_fraction();
    let to_bevy = Quat::from_rotation_x(-FRAC_PI_2);
    for o in &b.list {
        let (active, inactive, model, speed) = if o.dragon {
            (o.dragon_body.as_ref(), o.bird.as_ref(), b.dragon.as_ref(), DRAGON_PLAY_SPEED)
        } else {
            (o.bird.as_ref(), o.dragon_body.as_ref(), b.model.as_ref(), 1.0)
        };
        if let Some(body) = inactive
            && let Ok((_, mut vis)) = q.get_mut(body.entity)
        {
            *vis = Visibility::Hidden;
        }
        let (Some(body), Some(model)) = (active, model) else { continue };
        let Ok((mut tf, mut vis)) = q.get_mut(body.entity) else { continue };
        if !o.live {
            *vis = Visibility::Hidden;
            crate::shadow::Builder::new(None, Vec3::ZERO, Quat::IDENTITY, 1.0).finish(&o.shadow, &mut meshes);
            continue;
        }
        *vis = Visibility::Inherited;
        let pos = o.prev_pos.lerp(o.pos, a);
        let delta = (o.angle - o.prev_angle + 540.0).rem_euclid(360.0) - 180.0;
        let angle = o.prev_angle + delta * a;
        // El original los dibuja girados un cuarto de vuelta.
        *tf = Transform {
            translation: Vec3::new(pos.x, pos.z, -pos.y) / MU_SCALE,
            rotation: to_bevy * Quat::from_rotation_z((angle + 90.0).to_radians()),
            scale: Vec3::splat(o.scale / MU_SCALE),
        };
        let n = frames(Some(model), o.action);
        let bones = model.bone_matrices_at(o.action, (o.frame + a * speed) % n);
        let mut sh = crate::shadow::Builder::new(
            terrain.as_deref().map(|t| &t.0),
            pos,
            Quat::from_rotation_z((angle + 90.0).to_radians()),
            o.scale,
        );
        for (mi, h) in &body.parts {
            let posed = model.pose_with(*mi, &bones);
            sh.add(&posed.positions);
            if let Some(mut m) = meshes.get_mut(h) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
            }
        }
        sh.finish(&o.shadow, &mut meshes);
        // Luz: la del suelo bajo el bicho (las mariposas brillan por sí mismas). Los dragones
        // dorados llevan un brillo metálico en el original; aquí, un tono dorado aproximado.
        let color = if o.dragon && o.golden {
            Some(Color::linear_rgb(1.6, 1.2, 0.45))
        } else if b.kind == Some(Kind::Butterfly) && !o.dragon {
            None
        } else {
            terrain.as_ref().map(|t| {
                let x = (pos.x / MU_SCALE).clamp(0.0, 255.0) as usize;
                let y = (pos.y / MU_SCALE).clamp(0.0, 255.0) as usize;
                let l = t.0.light[y * SIZE + x].map(|v| v.clamp(0.0, 1.0).powf(2.2));
                Color::linear_rgb(l[0], l[1], l[2])
            })
        };
        if let Some(color) = color {
            for h in &body.materials {
                if let Some(mut m) = materials.get_mut(h) {
                    if m.base_color != color {
                        m.base_color = color;
                    }
                }
            }
        }
    }
}
