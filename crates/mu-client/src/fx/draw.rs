//! Per-frame effect drawing: interpolates between the last two ticks, asks each kind what it
//! draws (`DescribeParticle`, `RenderEffects`...) and groups by texture and blending into meshes
//! rebuilt every frame.

use std::cell::RefCell;
use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::core::*;
use super::material::FxMaterial;
use super::sim::{Actors, FxWorld};
use super::textures::FxTextures;
use crate::terrain::{LoadedTerrain, MU_SCALE};

/// What to draw this frame (also used by models, in `models.rs`).
#[derive(Resource, Default)]
pub struct FrameDraw(pub DrawList);

/// Step 1: ask each object what it draws, already interpolated.
pub fn collect(
    world: Res<FxWorld>,
    registry: Res<Registry>,
    actors: Res<Actors>,
    fixed: Res<Time<Fixed>>,
    mut textures: ResMut<FxTextures>,
    mut images: ResMut<Assets<Image>>,
    mut frame: ResMut<FrameDraw>,
    args: Res<crate::Args>,
    terrain: Option<Res<LoadedTerrain>>,
    lights: Res<crate::terrain::TerrainLights>,
    options: Option<Res<crate::options::GameOptions>>,
) {
    let a = fixed.overstep_fraction();
    let tex = RefCell::new((&mut *textures, &mut *images));
    let tex_info = |id: i32| {
        let mut t = tex.borrow_mut();
        let (textures, images) = &mut *t;
        textures.info(id, images)
    };
    let ctx = DrawCtx {
        world_time: fixed.elapsed_secs() * 1000.0,
        tex_info: &tex_info,
        actors: &actors.map,
        world_active: args.world as i32 - 1,
        models: &world.models,
        terrain: terrain.as_deref().map(|t| (&t.0, &*lights)),
        render_level: options.map_or(crate::options::GameOptions::default().render_level, |o| o.render_level),
    };
    let mut out = DrawList::default();

    for p in &world.particles {
        let mut q = p.clone();
        q.position = p.prev_position.lerp(p.position, a);
        q.scale = p.prev_scale + (p.scale - p.prev_scale) * a;
        q.light = p.prev_light.lerp(p.light, a);
        match registry.particles.get(&p.ty).and_then(|k| k.draw) {
            Some(draw) => draw(&q, &ctx, &mut out),
            None => default_particle_draw(&q, &ctx, &mut out),
        }
    }

    // `DescribeSprite`: the subtype picks the blending.
    for s in &world.sprites {
        let Some(t) = ctx.tex(s.ty) else { continue };
        let blend = match s.sub_type {
            1 => Blend::Subtract,
            2 => Blend::AlphaTest,
            3 => Blend::Blend2,
            _ => Blend::Glow,
        };
        out.sprites.push(SpriteDraw {
            texture: s.ty,
            position: s.position,
            width: t.width * s.scale,
            height: t.height * s.scale,
            color: [s.light.x, s.light.y, s.light.z, if t.components == 3 { s.alpha } else { s.light.x * s.alpha }],
            rotation: s.rotation,
            uv: [0.0, 0.0, 1.0, 1.0],
            blend,
        });
    }

    // Weapon trails (`RenderBlurs`): additive, fading toward the oldest.
    for b in &world.blurs {
        let n = b.p1.len();
        if n < 2 {
            continue;
        }
        let tex = b.texture();
        for j in 0..n - 1 {
            let (l1, l2) = ((n - j) as f32 / n as f32, (n - j - 1) as f32 / n as f32);
            let (u1, u2) = (j as f32 / n as f32, (j + 1) as f32 / n as f32);
            // One color per quad: the average of its two ends.
            let l = b.light * (l1 + l2) * 0.5;
            out.quads.push(QuadDraw {
                texture: tex,
                corners: [b.p1[j], b.p2[j], b.p2[j + 1], b.p1[j + 1]],
                uv: [[u1, 1.0], [u1, 0.0], [u2, 0.0], [u2, 1.0]],
                color: [l.x, l.y, l.z, 1.0],
                blend: Blend::Glow,
            });
        }
    }

    for e in &world.effects {
        let mut q = e.clone();
        q.position = e.prev_position.lerp(e.position, a);
        let d = |x: f32, y: f32| (y - x + 540.0).rem_euclid(360.0) - 180.0;
        q.angle = Vec3::new(
            e.prev_angle.x + d(e.prev_angle.x, e.angle.x) * a,
            e.prev_angle.y + d(e.prev_angle.y, e.angle.y) * a,
            e.prev_angle.z + d(e.prev_angle.z, e.angle.z) * a,
        );
        match registry.effects.get(&e.ty).and_then(|k| k.draw) {
            Some(draw) => draw(&q, &ctx, &mut out),
            None if e.visible => out.models.push(ModelDraw::of_effect(&q)),
            None => {}
        }
    }
    frame.0 = out;
}

/// Sprite/trail/mark meshes already created, by (texture, blending).
#[derive(Resource, Default)]
pub struct Batches {
    /// (mesh, material, entity) of each texture + blending batch.
    map: HashMap<(i32, Blend), (Handle<Mesh>, Handle<FxMaterial>, Entity)>,
}

#[derive(Default)]
struct Batch {
    pos: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    color: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Batch {
    fn quad(&mut self, p: [Vec3; 4], uv: [[f32; 2]; 4], color: [f32; 4]) {
        let base = self.pos.len() as u32;
        for (k, v) in p.iter().enumerate() {
            self.pos.push(mu_to_bevy(*v).to_array());
            self.uv.push(uv[k]);
            self.color.push(color);
        }
        self.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// MU (x, y, z up, 100 per tile) → Bevy (x, z, -y), 1 per tile.
pub fn mu_to_bevy(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, -v.y) / MU_SCALE
}

/// Original color (multiplied in sRGB) → linear. Like `PackABGR`, clamped to [0, 1].
pub(crate) fn linear(c: [f32; 4]) -> [f32; 4] {
    let f = |v: f32| v.clamp(0.0, 1.0).powf(2.2);
    [f(c[0]), f(c[1]), f(c[2]), c[3].clamp(0.0, 1.0)]
}

/// Step 2: sprites, marks and trails into meshes.
#[allow(clippy::too_many_arguments)]
pub fn render_quads(
    mut commands: Commands,
    world: Res<FxWorld>,
    frame: Res<FrameDraw>,
    fixed: Res<Time<Fixed>>,
    terrain: Option<Res<LoadedTerrain>>,
    cameras: Query<&Transform, (With<Camera3d>, With<IsDefaultUiCamera>)>,
    mut batches: ResMut<Batches>,
    mut textures: ResMut<FxTextures>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FxMaterial>>,
    mut visibility: Query<&mut Visibility>,
) {
    let Ok(cam) = cameras.single() else { return };
    // Screen axes in MU coordinates (for sprites).
    let to_mu = |v: Vec3| Vec3::new(v.x, -v.z, v.y);
    let (right, up) = (to_mu(cam.right().as_vec3()), to_mu(cam.up().as_vec3()));
    let mut out: HashMap<(i32, Blend), Batch> = HashMap::new();

    for s in &frame.0.sprites {
        let (sn, cs) = s.rotation.to_radians().sin_cos();
        let (w, h) = (s.width * 0.5, s.height * 0.5);
        let [u, v, uw, vh] = s.uv;
        // Same as `RenderSprite`: bottom-left, bottom-right, top-right, top-left; v = 0 at the top.
        let corners = [(-w, -h), (w, -h), (w, h), (-w, h)].map(|(dx, dy)| {
            s.position + right * (dx * cs - dy * sn) + up * (dx * sn + dy * cs)
        });
        let uvs = [[u, v + vh], [u + uw, v + vh], [u + uw, v], [u, v]];
        out.entry((s.texture, s.blend)).or_default().quad(corners, uvs, linear(s.color));
    }

    let height = |x: f32, y: f32| terrain.as_ref().map_or(0.0, |t| t.0.height_at(x / MU_SCALE, y / MU_SCALE));
    for d in &frame.0.decals {
        let (sn, cs) = d.rotation.to_radians().sin_cos();
        let (w, h) = (d.width * 0.5, d.height * 0.5);
        let corners = [(-w, -h), (w, -h), (w, h), (-w, h)].map(|(dx, dy)| {
            let x = d.center.x + dx * cs - dy * sn;
            let y = d.center.y + dx * sn + dy * cs;
            Vec3::new(x, y, height(x, y) + d.lift)
        });
        // Like `RenderTerrainAlphaBitmap`: V grows toward +Y.
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        out.entry((d.texture, d.blend)).or_default().quad(corners, uvs, linear(d.color));
    }

    for qd in &frame.0.quads {
        out.entry((qd.texture, qd.blend)).or_default().quad(qd.corners, qd.uv, linear(qd.color));
    }

    // Trails (`DescribeJoint`): two crossed ribbons along the tails.
    let scroll = (fixed.elapsed_secs() * 1000.0 % 1000.0) * 0.001;
    for j in &world.joints {
        let n = j.tails.len();
        if n < 2 {
            continue;
        }
        let blend = match j.render_type {
            JointRender::AlphaBlend => Blend::Glow,
            JointRender::AlphaTest => Blend::AlphaTest,
            JointRender::AlphaBlendMinus => Blend::Subtract,
            JointRender::AlphaBlendOther => Blend::Blend2,
        };
        let max = (j.max_tails.max(2) - 1) as f32;
        let color = linear([j.light.x, j.light.y, j.light.z, 1.0]);
        let u_of = |k: usize, face_one: bool| -> f32 {
            let base = match j.reverse_uv {
                3 => 1.0 - k as f32 / max,
                _ => (n - 1 - k) as f32 / max,
            };
            let base = if j.reverse_uv == 2 { 1.0 - base } else { base };
            match j.scroll {
                JointScroll::None => base,
                JointScroll::Thunder => {
                    // Face one: U·2 − scroll; face two: the same plus 2·scroll (= U·2 + scroll).
                    if face_one { base * 2.0 - scroll } else { base * 2.0 + scroll }
                }
                JointScroll::Tile => (n - 1 - k) as f32 / 16.0 * 2.0 - 2.0 * scroll,
                // FLARE_FORCE divides by (MaxTails − 1) / 2 with integer division.
                JointScroll::Flare => (n - 1 - k) as f32 / ((j.max_tails.max(3) - 1) / 2) as f32 - scroll,
            }
        };
        let (v0, v1) = if j.reverse_uv == 1 { (1.0, 0.0) } else { (0.0, 1.0) };
        let batch = out.entry((j.tex_type, blend)).or_default();
        for k in 0..n - 1 {
            let (a, b) = (&j.tails[k], &j.tails[k + 1]);
            if j.render_face & RENDER_FACE_ONE != 0 {
                let (ua, ub) = (u_of(k, true), u_of(k + 1, true));
                batch.quad([a[2], a[3], b[3], b[2]], [[ua, v0], [ua, v1], [ub, v1], [ub, v0]], color);
            }
            if j.render_face & RENDER_FACE_TWO != 0 {
                let (ua, ub) = (u_of(k, false), u_of(k + 1, false));
                batch.quad([a[0], a[1], b[1], b[0]], [[ua, v0], [ua, v1], [ub, v1], [ub, v0]], color);
            }
        }
    }

    // Upload into the meshes (creating missing ones). Batches with nothing this frame are hidden
    // instead of emptied: Bevy warns about every empty mesh it uploads.
    for (key, (_, _, entity)) in batches.map.iter() {
        if let Ok(mut v) = visibility.get_mut(*entity) {
            v.set_if_neq(if out.contains_key(key) { Visibility::Inherited } else { Visibility::Hidden });
        }
    }
    for (key, b) in out {
        let entry = batches.map.entry(key).or_insert_with(|| {
            let texture = textures.get(key.0, &mut images).map(|t| t.0);
            let mesh = meshes.add(empty_mesh());
            let material = materials.add(FxMaterial::new(texture, key.1));
            let entity = commands.spawn((
                Name::new(format!("Efectos {} {:?}", key.0, key.1)),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                // The mesh changes every frame: no bounding-box culling (always drawn).
                bevy::camera::visibility::NoFrustumCulling,
            )).id();
            (mesh, material, entity)
        });
        if let Some(mut m) = meshes.get_mut(&entry.0) {
            let n = b.pos.len();
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, b.pos);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n]);
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, b.uv);
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, b.color);
            m.insert_indices(Indices::U32(b.idx));
        }
    }
}

pub fn empty_mesh() -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new())
        .with_inserted_indices(Indices::U32(vec![]))
}
