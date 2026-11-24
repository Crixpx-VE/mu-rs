//! Effect `.bmd` models (`RenderObject` of `Effects[]`): spinning swords, cracks, waves,
//! meteors... Each model has a pool of instances reused every frame; each [`ModelDraw`] takes its
//! own instance (one `RenderBody`/`RenderMesh` call), so several draws of one model in a frame can
//! give its meshes different flags, lights or textures.
//!
//! Per-mesh rules ([`resolve`], like `BMD::RenderBody` + `BMD::RenderMesh`, ZzzBMD.cpp:937-1354,
//! 1851-1913):
//! - never drawn: meshes whose texture name starts with `hid` (`BITMAP_HIDE`, LoadData.cpp:73-74,
//!   ZzzBMD.cpp:947-948), and all meshes with `hidden_mesh` = -2.
//! - `mesh` = -1 (`RenderBody`): `hidden_mesh` = n hides the mesh with texture n; texture script `_H`
//!   hides, `_R` makes the mesh its own `BlendMesh` (ZzzBMD.cpp:1869-1877). `mesh` = i
//!   (`RenderMesh(i)`): only mesh i, without those rules.
//! - `texture` ≠ -1 (`MeshTexture`) replaces the mesh's texture (ZzzBMD.cpp:964-965).
//! - `render_flags` = 0 keeps the engine's historical rules:
//!   - `bright`: the whole model additive with `light · alpha`.
//!   - the mesh whose texture is `blend_mesh` (all with ≤ -2): additive with
//!     `light · blend_mesh_light` and offset UVs.
//!   - the rest: texture with alpha → alpha test; alpha < 1 → normal blend; otherwise opaque.
//! - `render_flags` ≠ 0 is the original `RenderFlag` verbatim (`bright` adds
//!   `RENDER_TEXTURE|RENDER_BRIGHT`), resolved in `RenderMesh`'s order:
//!   1. `RENDER_COLOR`: untextured `BodyLight`; `BRIGHT` additive, `DARK` subtractive, else
//!      opaque; alpha < 0.99 → alpha test with that alpha (ZzzBMD.cpp:996-1021).
//!   2. `RENDER_CHROME*`/`METAL`: environment UVs from the posed world normals and `WorldTime`
//!      (ZzzBMD.cpp:1051-1108), `BITMAP_CHROME`/`CHROME2`/`CHROME6`/`SHINY` (ZzzBMD.cpp:1148-1169),
//!      flat `BodyLight`; CHROME3/4/5/7 or `BRIGHT` additive (and `BodyLight *= Alpha` below 0.99,
//!      cumulative over the meshes of the call), `DARK` subtractive, `LIGHTMAP` multiply, else
//!      opaque / alpha test (ZzzBMD.cpp:1110-1141). `_N` meshes are skipped (ZzzBMD.cpp:1033-1039).
//!   3. the `BlendMesh`: additive (`DARK`: subtractive) with `BodyLight · BlendMeshLight`
//!      (ZzzBMD.cpp:1171-1188).
//!   4. `RENDER_TEXTURE`: `BRIGHT` additive, `DARK` subtractive, alpha < 0.99 or a TGA → alpha
//!      test, else opaque (ZzzBMD.cpp:1189-1214).
//!   5. `RENDER_BRIGHT` alone: untextured additive, skipped on TGA meshes (ZzzBMD.cpp:1215-1231).
//!
//!   `RENDER_NODEPTH` disables the depth test; `RENDER_WAVE` pushes vertices along their normals
//!   (ZzzBMD.cpp:1331-1346). The UV offset is added when the mesh is the `BlendMesh` or a `_S`
//!   stream mesh (`EnableWave`, ZzzBMD.cpp:978-979) or with `RENDER_CHROME4` (ZzzBMD.cpp:1301).
//!   Not reproduced: `RENDER_OIL` (not drawn, warned once), `RENDER_DOPPELGANGER`'s face culling,
//!   per-vertex terrain lighting (`LightTransform`).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::bmd::{self, Bmd, Mat34, TextureScript};
use mu_data::fs;
use mu_data::objects::render::{ChromeMode, chrome_mode, chrome_uv};
use mu_data::texture::{self, AlphaKind};

use super::core::*;
use super::draw::{FrameDraw, mu_to_bevy};
use super::material::{FxMaterial, FxParams};
use super::textures::{FxTextures, fx_image};
use crate::terrain::MU_SCALE;

/// Every flag that takes `RenderMesh`'s chrome branch (ZzzBMD.cpp:1022-1031).
const ANY_CHROME: u32 = RENDER_CHROME
    | RENDER_CHROME2
    | RENDER_CHROME3
    | RENDER_CHROME4
    | RENDER_CHROME5
    | RENDER_CHROME6
    | RENDER_CHROME7
    | RENDER_METAL
    | RENDER_OIL;

/// What is known about a mesh of a loaded model.
struct MeshData {
    texture: Option<Handle<Image>>,
    /// The texture has transparency (the historical rules).
    has_alpha: bool,
    /// `pBitmap->Components == 4`: the texture is a TGA (`LoadData.cpp:78-79`).
    components4: bool,
    script: Option<TextureScript>,
    /// `BITMAP_HIDE`.
    hide: bool,
    /// Original vertex index of each posed corner (`tp->VertexIndex[k]`, for `RENDER_WAVE`).
    corner_vertex: Vec<u32>,
}

struct LoadedModel {
    bmd: Arc<Bmd>,
    meshes: Vec<MeshData>,
}

struct Part {
    mesh_index: usize,
    texture_index: i32,
    mesh: Handle<Mesh>,
    material: Handle<FxMaterial>,
    entity: Entity,
    /// Rest pose (action 0, frame 0) of the mesh.
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    /// The mesh holds chrome UVs instead of the model's.
    uv_changed: bool,
    /// The mesh holds `RENDER_WAVE` positions instead of the rest pose.
    positions_changed: bool,
}

struct Instance {
    root: Entity,
    parts: Vec<Part>,
}

#[derive(Resource)]
pub struct FxModels {
    data: PathBuf,
    /// `gMapManager.WorldActive` the loaded models belong to.
    world: i32,
    loaded: HashMap<i32, Option<Arc<LoadedModel>>>,
    pools: HashMap<i32, Vec<Instance>>,
    /// Unsupported flag combinations already reported.
    warned: HashSet<u32>,
}

impl FxModels {
    pub fn new(data: PathBuf) -> Self {
        Self {
            data,
            world: 0,
            loaded: HashMap::new(),
            pools: HashMap::new(),
            warned: HashSet::new(),
        }
    }

    /// Folder, file and texture folder of model `id` on the current map ([`super::model_data::locate`]).
    fn locate(&self, id: i32) -> Option<(String, String, String)> {
        super::model_data::locate(id, self.world)
    }

    /// On another map the per-map models (`MAP_MODELS`) may be other files: drop every loaded
    /// model and its instances.
    fn set_world(&mut self, world_active: i32, commands: &mut Commands) {
        if self.world == world_active {
            return;
        }
        self.world = world_active;
        self.loaded.clear();
        for inst in self.pools.drain().flat_map(|(_, p)| p) {
            commands.entity(inst.root).despawn();
        }
    }

    fn load(&mut self, id: i32, images: &mut Assets<Image>) -> Option<Arc<LoadedModel>> {
        if let Some(m) = self.loaded.get(&id) {
            return m.clone();
        }
        let result = self.locate(id).and_then(|(dir, file, texdir)| {
            let d = self.data.join(&dir);
            let bmd = match fs::read(&d, &file).and_then(|b| bmd::read(&b)) {
                Ok(b) => Arc::new(b),
                Err(e) => {
                    warn!("efecto: no se pudo cargar el modelo {id} ({dir}/{file}): {e}");
                    return None;
                }
            };
            let td = self.data.join(&texdir);
            let meshes = bmd
                .meshes
                .iter()
                .map(|m| {
                    let file = bmd::texture_file_name(&m.texture);
                    let (texture, has_alpha) = match texture::load(&td, &file) {
                        Ok(t) => (Some(images.add(fx_image(&t, true))), t.alpha_kind() != AlphaKind::Opaque),
                        Err(_) => (None, false),
                    };
                    let name = m.texture.rsplit(['\\', '/']).next().unwrap_or(&m.texture);
                    MeshData {
                        texture,
                        has_alpha,
                        // `tolower(__ext[1]) == 't'` → TGA, loaded with 4 components (LoadData.cpp:78-79).
                        components4: file.to_ascii_lowercase().ends_with(".ozt"),
                        script: bmd::texture_script(&m.texture),
                        // Case-sensitive, like the original's byte compares (LoadData.cpp:73).
                        hide: name.starts_with("hid"),
                        corner_vertex: corner_vertices(m),
                    }
                })
                .collect();
            Some(Arc::new(LoadedModel { bmd, meshes }))
        });
        self.loaded.insert(id, result.clone());
        result
    }
}

/// Where a mesh's texture comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TexSource {
    /// The mesh's own.
    Mesh,
    /// A `BITMAP_*`.
    Bitmap(i32),
}

/// What [`resolve`] needs about a mesh.
#[derive(Debug, Clone, Copy)]
struct MeshCtx {
    index: i32,
    /// `m->Texture`.
    texture_index: i32,
    script: Option<TextureScript>,
    hide: bool,
    /// Transparency of the texture used (historical rules).
    has_alpha: bool,
    /// `pBitmap->Components == 4` of `Texture` (the mesh's, or `MeshTexture`).
    components4: bool,
}

/// How a mesh is drawn by one [`ModelDraw`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct Look {
    blend: Blend,
    texture: TexSource,
    untextured: bool,
    /// Chrome UVs instead of the model's.
    chrome: Option<ChromeMode>,
    uv_offset: Vec2,
    /// `glColor` (original sRGB values).
    color: Vec3,
    alpha: f32,
    depth_write: Option<bool>,
    no_depth_test: bool,
    /// `RENDER_WAVE`.
    wave: bool,
}

/// The chrome formula of `flags`, or `None` for `RENDER_OIL` (not translated). The order of the
/// original `if`s puts OIL before CHROME and after CHROME2..7 (ZzzBMD.cpp:1059-1107).
fn chrome_formula(flags: u32) -> Option<ChromeMode> {
    let before_oil =
        RENDER_CHROME2 | RENDER_CHROME3 | RENDER_CHROME4 | RENDER_CHROME5 | RENDER_CHROME6 | RENDER_CHROME7;
    if flags & RENDER_OIL != 0 && flags & before_oil == 0 {
        return None;
    }
    chrome_mode(flags)
}

/// Why `flags` cannot be drawn as the original does (reported once).
fn unsupported(flags: u32) -> Option<&'static str> {
    if flags & ANY_CHROME != 0 && chrome_formula(flags).is_none() {
        Some("RENDER_OIL")
    } else {
        None
    }
}

/// How mesh `m` is drawn by `d` (`None` = not drawn). `light_scale` carries `BodyLight *= Alpha`
/// from one mesh to the next within the same call (ZzzBMD.cpp:1116-1128 modify the model's
/// `BodyLight` itself); start it at 1 for each [`ModelDraw`].
fn resolve(d: &ModelDraw, m: &MeshCtx, light_scale: &mut f32) -> Option<Look> {
    if m.hide || d.hidden_mesh == -2 {
        return None;
    }
    let mut blend_mesh = d.blend_mesh;
    if d.mesh >= 0 {
        if d.mesh != m.index {
            return None;
        }
    } else {
        if d.hidden_mesh == m.texture_index || m.script.is_some_and(|s| s.hidden) {
            return None;
        }
        if m.script.is_some_and(|s| s.bright) {
            blend_mesh = m.index;
        }
    }
    let texture = if d.texture != -1 { TexSource::Bitmap(d.texture) } else { TexSource::Mesh };
    let tc = d.blend_mesh_tex_coord;
    let stream = m.script.is_some_and(|s| s.stream);
    let is_blend_mesh = blend_mesh <= -2 || m.texture_index == blend_mesh;
    let base = Look {
        blend: Blend::Opaque,
        texture,
        untextured: false,
        chrome: None,
        uv_offset: Vec2::ZERO,
        color: d.light,
        alpha: d.alpha,
        depth_write: None,
        no_depth_test: false,
        wave: false,
    };

    if d.render_flags == 0 {
        // The engine's historical rules (see the module comment).
        return Some(if d.bright {
            Look { blend: Blend::Glow, color: d.light * d.alpha, alpha: 1.0, ..base }
        } else if is_blend_mesh {
            Look { blend: Blend::Glow, color: d.light * d.blend_mesh_light, alpha: 1.0, uv_offset: tc, ..base }
        } else {
            let blend = if m.has_alpha {
                Blend::AlphaTest
            } else if d.alpha < 0.99 {
                Blend::Blend3
            } else {
                Blend::Opaque
            };
            Look { blend, uv_offset: if stream { tc } else { Vec2::ZERO }, ..base }
        });
    }

    let mut f = d.render_flags;
    if d.bright {
        f |= RENDER_TEXTURE | RENDER_BRIGHT;
    }
    let has = |flag: u32| f & flag != 0;
    let translucent = d.alpha < 0.99;
    // `glColor3fv(BodyLight)` at 0.99 or more, `glColor4f(..., Alpha)` below.
    let base = Look {
        alpha: if translucent { d.alpha } else { 1.0 },
        no_depth_test: has(RENDER_NODEPTH),
        wave: has(RENDER_WAVE),
        ..base
    };
    // `EnableWave` (ZzzBMD.cpp:978-979): `i == BlendMesh` (the mesh index) or the `_S` stream mesh.
    let wave_uv = if (m.index == blend_mesh || stream) && tc != Vec2::ZERO { tc } else { Vec2::ZERO };

    let look = if has(RENDER_COLOR) {
        // `EnableAlphaTest` below 0.99 replaces the blending chosen before (ZzzBMD.cpp:1012-1020),
        // and `DisableTexture` turns depth writes back on (ZzzOpenglUtil.cpp:394-396).
        let blend = if translucent {
            Blend::AlphaTest
        } else if has(RENDER_BRIGHT) {
            Blend::Glow
        } else if has(RENDER_DARK) {
            Blend::Subtract
        } else {
            Blend::Opaque
        };
        Look { blend, untextured: true, depth_write: Some(true), ..base }
    } else if has(ANY_CHROME) {
        if m.script.is_some_and(|s| s.none_blend) {
            return None;
        }
        let mode = chrome_formula(f)?;
        let blend = if has(RENDER_CHROME3 | RENDER_CHROME4 | RENDER_CHROME5 | RENDER_CHROME7) || has(RENDER_BRIGHT) {
            if translucent {
                *light_scale *= d.alpha;
            }
            Blend::Glow
        } else if has(RENDER_DARK) {
            Blend::Subtract
        } else if has(RENDER_LIGHTMAP) {
            Blend::LightMap
        } else if !translucent {
            Blend::Opaque
        } else {
            Blend::AlphaTest
        };
        let texture = if d.texture != -1 {
            texture
        } else if has(RENDER_CHROME2 | RENDER_CHROME3 | RENDER_CHROME4) {
            TexSource::Bitmap(BITMAP_CHROME2)
        } else if has(RENDER_CHROME6) {
            TexSource::Bitmap(BITMAP_CHROME6)
        } else if has(RENDER_CHROME) {
            TexSource::Bitmap(BITMAP_CHROME)
        } else if has(RENDER_METAL) {
            TexSource::Bitmap(BITMAP_SHINY)
        } else {
            TexSource::Mesh
        };
        let uv_offset = if mode == ChromeMode::Chrome4 { tc } else { Vec2::ZERO };
        Look { blend, texture, chrome: Some(mode), uv_offset, color: d.light * *light_scale, ..base }
    } else if is_blend_mesh {
        let blend = if has(RENDER_DARK) { Blend::Subtract } else { Blend::Glow };
        Look { blend, color: d.light * d.blend_mesh_light, alpha: 1.0, uv_offset: wave_uv, ..base }
    } else if has(RENDER_TEXTURE) {
        let blend = if has(RENDER_BRIGHT) {
            Blend::Glow
        } else if has(RENDER_DARK) {
            Blend::Subtract
        } else if translucent || m.components4 {
            Blend::AlphaTest
        } else {
            Blend::Opaque
        };
        Look { blend, uv_offset: wave_uv, ..base }
    } else if has(RENDER_BRIGHT) {
        // (`m->Texture == BlendMesh` was already taken by the blend mesh branch.)
        if m.components4 {
            return None;
        }
        // `DisableDepthMask` after `DisableTexture` (ZzzBMD.cpp:1223-1225).
        Look { blend: Blend::Glow, untextured: true, depth_write: Some(false), ..base }
    } else {
        // `Render = RENDER_TEXTURE` with whatever blending the previous draw left (ZzzBMD.cpp:1232-1235):
        // not reproducible, drawn like `RENDER_TEXTURE` at full alpha.
        let blend = if m.components4 { Blend::AlphaTest } else { Blend::Opaque };
        Look { blend, uv_offset: wave_uv, no_depth_test: false, ..base }
    };
    // Fixed-function OpenGL clamps vertex colors to [0, 1].
    Some(Look { color: look.color.clamp(Vec3::ZERO, Vec3::ONE), ..look })
}

/// Original vertex index of each corner [`Bmd::pose_with`] emits (same loop, same skipped corners).
fn corner_vertices(m: &bmd::Mesh) -> Vec<u32> {
    let mut out = vec![];
    for t in &m.triangles {
        let corners: &[usize] = if t.polygon == 4 { &[0, 1, 2, 0, 2, 3] } else { &[0, 1, 2] };
        for &c in corners {
            let ok = m.vertices.get(t.vertex[c] as usize).is_some()
                && m.normals.get(t.normal[c] as usize).is_some()
                && m.uvs.get(t.uv[c] as usize).is_some();
            if ok {
                out.push(t.vertex[c] as u32);
            }
        }
    }
    out.truncate(out.len() / 3 * 3);
    out
}

/// Chrome UVs (`g_chrome`, ZzzBMD.cpp:1054-1108) of posed model-space normals.
///
/// The original uses `NormalTransform`, rotated by the bones *and* by `ParentMatrix`: effects go
/// through `RenderObject(o)` with `Translate = false` (ZzzObject.h:21), so `Animation(..., !Translate)`
/// puts `AngleMatrix(Angle) · BodyScale` into `ParentMatrix` (ZzzObject.cpp:285-289,
/// ZzzBMD.cpp:147-166) and the world normals are `scale` long.
fn chrome_uvs(normals: &[[f32; 3]], rotation: &Mat34, scale: f32, mode: ChromeMode, world_time: f32, offset: Vec2) -> Vec<[f32; 2]> {
    normals
        .iter()
        .map(|n| {
            let w = bmd::rotate_vector(rotation, *n).map(|v| v * scale);
            let uv = chrome_uv(mode, w, world_time);
            [uv[0] + offset.x, uv[1] + offset.y]
        })
        .collect()
}

/// `RENDER_WAVE` (ZzzBMD.cpp:1331-1346): each vertex moves `sin(((int)WorldTime + vi·931) · 0.007) · 28`
/// along its world normal. That normal is `scale` long and the model is drawn scaled by `scale`,
/// so in model space the displacement is the posed normal times `sin · 28`.
fn wave_positions(positions: &[[f32; 3]], normals: &[[f32; 3]], vertex: &[u32], world_time: f32) -> Vec<[f32; 3]> {
    let wt = world_time as i32;
    positions
        .iter()
        .zip(normals)
        .enumerate()
        .map(|(k, (p, n))| {
            let vi = vertex.get(k).copied().unwrap_or(0) as i32;
            let s = (wt.wrapping_add(vi.wrapping_mul(931)) as f32 * 0.007).sin() * 28.0;
            [p[0] + n[0] * s, p[1] + n[1] * s, p[2] + n[2] * s]
        })
        .collect()
}

/// Step 3: effect models.
#[allow(clippy::too_many_arguments)]
pub fn render_models(
    mut commands: Commands,
    frame: Res<FrameDraw>,
    fixed: Res<Time<Fixed>>,
    mut models: ResMut<FxModels>,
    mut textures: ResMut<FxTextures>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FxMaterial>>,
    mut q: Query<(&mut Transform, &mut Visibility)>,
    args: Res<crate::Args>,
) {
    models.set_world(args.world as i32 - 1, &mut commands);
    let mut used: HashMap<i32, usize> = HashMap::new();
    let to_bevy_rot = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    // `WorldTime` in ms, continuous between ticks (the chrome UVs move every frame in the original).
    let world_time = ((fixed.elapsed() + fixed.overstep()).as_secs_f64() * 1000.0) as f32;

    for d in &frame.0.models {
        let Some(model) = models.load(d.model, &mut images) else { continue };
        if let Some(what) = unsupported(d.render_flags)
            && models.warned.insert(d.render_flags)
        {
            warn!("efecto: modelo {} con {what} (flags {:#x}): no implementado", d.model, d.render_flags);
        }
        let slot = used.entry(d.model).or_insert(0);
        let index = *slot;
        *slot += 1;

        let pool = models.pools.entry(d.model).or_default();
        if pool.len() <= index {
            pool.push(spawn_instance(&mut commands, &model, d.model, &mut meshes, &mut materials));
        }
        let inst = &mut pool[index];

        let a = d.angle * (std::f32::consts::PI / 180.0);
        if let Ok((mut tf, mut vis)) = q.get_mut(inst.root) {
            *tf = Transform {
                translation: mu_to_bevy(d.position),
                rotation: to_bevy_rot * Quat::from_euler(EulerRot::ZYX, a.z, a.y, a.x),
                scale: Vec3::splat(d.scale / MU_SCALE),
            };
            *vis = Visibility::Inherited;
        }
        // `AngleMatrix(o->Angle)`: the same rotation as the transform above, in MU axes.
        let rotation = bmd::angle_matrix([a.x, a.y, a.z]);
        let override_tex = if d.texture != -1 { textures.get(d.texture, &mut images) } else { None };

        let animated = model.bmd.actions.get(d.action).is_some_and(|x| x.frames > 1);
        let bones = if animated { Some(model.bmd.bone_matrices_at(d.action, d.frame)) } else { None };
        let mut light_scale = 1.0;
        for part in &mut inst.parts {
            let data = &model.meshes[part.mesh_index];
            let components4 = match &override_tex {
                Some((_, info)) => info.components == 4,
                None => data.components4,
            };
            let ctx = MeshCtx {
                index: part.mesh_index as i32,
                texture_index: part.texture_index,
                script: data.script,
                hide: data.hide,
                has_alpha: if override_tex.is_some() { components4 } else { data.has_alpha },
                components4,
            };
            let look = resolve(d, &ctx, &mut light_scale);
            // The texture to bind (a missing `BITMAP_*` hides the mesh instead of drawing it white).
            let texture = match look.map(|l| (l.texture, l.untextured)) {
                None => None,
                Some((_, true)) | Some((TexSource::Mesh, _)) => Some(data.texture.clone()),
                Some((TexSource::Bitmap(id), false)) => match (&override_tex, d.texture == id) {
                    (Some((h, _)), true) => Some(Some(h.clone())),
                    _ => textures.get(id, &mut images).map(|t| Some(t.0)),
                },
            };
            let shown = look.is_some() && texture.is_some();
            if let Ok((_, mut vis)) = q.get_mut(part.entity) {
                vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
            }
            let (Some(look), Some(texture)) = (look, texture) else { continue };

            // Geometry: the pose, `RENDER_WAVE` and the chrome UVs.
            let posed = bones.as_ref().map(|b| model.bmd.pose_with(part.mesh_index, b));
            let (positions, normals) = match &posed {
                Some(p) => (&p.positions, &p.normals),
                None => (&part.positions, &part.normals),
            };
            let new_positions = if look.wave {
                Some(wave_positions(positions, normals, &data.corner_vertex, world_time))
            } else if posed.is_some() || part.positions_changed {
                Some(positions.clone())
            } else {
                None
            };
            let new_uvs = match look.chrome {
                Some(mode) => Some(chrome_uvs(normals, &rotation, d.scale, mode, world_time, look.uv_offset)),
                None if part.uv_changed => Some(part.uvs.clone()),
                None => None,
            };
            if (new_positions.is_some() || new_uvs.is_some() || posed.is_some())
                && let Some(mut m) = meshes.get_mut(&part.mesh)
            {
                if let Some(p) = new_positions {
                    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, p);
                }
                if let Some(p) = &posed {
                    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, p.normals.clone());
                }
                if let Some(uv) = new_uvs {
                    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
                }
            }
            part.positions_changed = look.wave;
            part.uv_changed = look.chrome.is_some();

            // Material: only touched when something changed.
            let lin = |v: f32| v.max(0.0).powf(2.2);
            let tint = Vec4::new(lin(look.color.x), lin(look.color.y), lin(look.color.z), look.alpha);
            // Chrome offsets are already in the UVs.
            let uv_offset = if look.chrome.is_some() { Vec2::ZERO } else { look.uv_offset };
            if let Some(m) = materials.get(&part.material)
                && (m.blend != look.blend
                    || m.params.tint != tint
                    || m.params.uv_offset != uv_offset
                    || m.texture != texture
                    || m.untextured != look.untextured
                    || m.depth_write != look.depth_write
                    || m.no_depth_test != look.no_depth_test)
                && let Some(mut m) = materials.get_mut(&part.material)
            {
                m.blend = look.blend;
                m.params = FxParams { tint, uv_offset, ..m.params };
                m.texture = texture;
                m.untextured = look.untextured;
                m.depth_write = look.depth_write;
                m.no_depth_test = look.no_depth_test;
            }
        }
    }

    // Hide instances unused this frame.
    for (id, pool) in &models.pools {
        let n = used.get(id).copied().unwrap_or(0);
        for inst in pool.iter().skip(n) {
            if let Ok((_, mut vis)) = q.get_mut(inst.root)
                && *vis != Visibility::Hidden
            {
                *vis = Visibility::Hidden;
            }
        }
    }
}

fn spawn_instance(
    commands: &mut Commands,
    model: &LoadedModel,
    id: i32,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<FxMaterial>,
) -> Instance {
    let mut parts = vec![];
    let root = commands.spawn((Name::new(format!("Efecto modelo {id}")), Transform::default(), Visibility::Hidden)).id();
    for (i, m) in model.bmd.meshes.iter().enumerate() {
        let posed = model.bmd.pose(i, 0, 0);
        if posed.positions.is_empty() {
            continue;
        }
        let mesh = meshes.add(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions.clone())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals.clone())
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs.clone()),
        );
        let texture = model.meshes.get(i).and_then(|t| t.texture.clone());
        let material = materials.add(FxMaterial::new(texture, Blend::Opaque));
        let entity = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), ChildOf(root))).id();
        parts.push(Part {
            mesh_index: i,
            texture_index: m.texture_index as i32,
            mesh,
            material,
            entity,
            positions: posed.positions,
            normals: posed.normals,
            uvs: posed.uvs,
            uv_changed: false,
            positions_changed: false,
        });
    }
    Instance { root, parts }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh(index: i32) -> MeshCtx {
        MeshCtx { index, texture_index: index, script: None, hide: false, has_alpha: false, components4: false }
    }

    fn draw(flags: u32) -> ModelDraw {
        let mut d = ModelDraw::new(MODEL_ITEM, Vec3::ZERO, Vec3::ZERO, 1.0);
        d.render_flags = flags;
        d
    }

    fn look(d: &ModelDraw, m: &MeshCtx) -> Option<Look> {
        resolve(d, m, &mut 1.0)
    }

    #[test]
    fn chrome_flags_pick_formula_texture_and_blend() {
        let m = mesh(0);
        let l = look(&draw(RENDER_CHROME | RENDER_BRIGHT), &m).unwrap();
        assert_eq!((l.blend, l.texture, l.chrome), (Blend::Glow, TexSource::Bitmap(BITMAP_CHROME), Some(ChromeMode::Chrome)));
        // CHROME2 alone: opaque with BITMAP_CHROME2.
        let l = look(&draw(RENDER_CHROME2), &m).unwrap();
        assert_eq!((l.blend, l.texture, l.chrome), (Blend::Opaque, TexSource::Bitmap(BITMAP_CHROME2), Some(ChromeMode::Chrome2)));
        // CHROME3/4 also bind BITMAP_CHROME2 and are always additive.
        for f in [RENDER_CHROME3, RENDER_CHROME4] {
            let l = look(&draw(f), &m).unwrap();
            assert_eq!((l.blend, l.texture), (Blend::Glow, TexSource::Bitmap(BITMAP_CHROME2)));
        }
        // CHROME5 and CHROME7 keep the mesh's texture (no BindTexture case for them).
        for f in [RENDER_CHROME5, RENDER_CHROME7] {
            let l = look(&draw(f), &m).unwrap();
            assert_eq!((l.blend, l.texture), (Blend::Glow, TexSource::Mesh));
        }
        assert_eq!(look(&draw(RENDER_CHROME6), &m).unwrap().texture, TexSource::Bitmap(BITMAP_CHROME6));
        let l = look(&draw(RENDER_METAL | RENDER_BRIGHT), &m).unwrap();
        assert_eq!((l.texture, l.chrome), (TexSource::Bitmap(BITMAP_SHINY), Some(ChromeMode::Metal)));
        // DARK, LIGHTMAP, alpha.
        assert_eq!(look(&draw(RENDER_CHROME | RENDER_DARK), &m).unwrap().blend, Blend::Subtract);
        assert_eq!(look(&draw(RENDER_CHROME | RENDER_LIGHTMAP), &m).unwrap().blend, Blend::LightMap);
        let mut d = draw(RENDER_CHROME);
        d.alpha = 0.5;
        let l = look(&d, &m).unwrap();
        assert_eq!((l.blend, l.alpha), (Blend::AlphaTest, 0.5));
        // `MeshTexture` beats the chrome textures.
        let mut d = draw(RENDER_CHROME | RENDER_BRIGHT);
        d.texture = BITMAP_CHROME + 1;
        assert_eq!(look(&d, &m).unwrap().texture, TexSource::Bitmap(BITMAP_CHROME + 1));
        // Only CHROME4 adds BlendMeshTexCoordU/V.
        let mut d = draw(RENDER_CHROME4);
        d.blend_mesh_tex_coord = Vec2::new(0.25, 0.5);
        assert_eq!(look(&d, &m).unwrap().uv_offset, Vec2::new(0.25, 0.5));
        d.render_flags = RENDER_CHROME;
        assert_eq!(look(&d, &m).unwrap().uv_offset, Vec2::ZERO);
        // `_N` meshes are skipped by the chrome passes only.
        let n = MeshCtx { script: Some(TextureScript { none_blend: true, ..Default::default() }), ..m };
        assert!(look(&draw(RENDER_CHROME | RENDER_BRIGHT), &n).is_none());
        assert!(look(&draw(RENDER_TEXTURE), &n).is_some());
        // RENDER_OIL is not translated.
        assert!(look(&draw(RENDER_OIL), &m).is_none());
        assert_eq!(unsupported(RENDER_OIL | RENDER_CHROME), Some("RENDER_OIL"));
        assert_eq!(unsupported(RENDER_OIL | RENDER_CHROME2), None);
    }

    #[test]
    fn chrome_bright_alpha_darkens_cumulatively() {
        let mut d = draw(RENDER_CHROME | RENDER_BRIGHT);
        d.alpha = 0.5;
        let mut scale = 1.0;
        let l0 = resolve(&d, &mesh(0), &mut scale).unwrap();
        let l1 = resolve(&d, &mesh(1), &mut scale).unwrap();
        assert_eq!((l0.color, l1.color), (Vec3::splat(0.5), Vec3::splat(0.25)));
    }

    #[test]
    fn texture_color_bright_dark_branches() {
        let m = mesh(0);
        let tga = MeshCtx { components4: true, ..m };
        assert_eq!(look(&draw(RENDER_TEXTURE), &m).unwrap().blend, Blend::Opaque);
        assert_eq!(look(&draw(RENDER_TEXTURE), &tga).unwrap().blend, Blend::AlphaTest);
        assert_eq!(look(&draw(RENDER_TEXTURE | RENDER_DARK), &m).unwrap().blend, Blend::Subtract);
        assert_eq!(look(&draw(RENDER_TEXTURE | RENDER_BRIGHT), &m).unwrap().blend, Blend::Glow);
        // RENDER_COLOR: untextured, depth writes back on, alpha test below 0.99.
        let l = look(&draw(RENDER_COLOR | RENDER_BRIGHT), &m).unwrap();
        assert_eq!((l.blend, l.untextured, l.depth_write), (Blend::Glow, true, Some(true)));
        let mut d = draw(RENDER_COLOR | RENDER_DARK);
        assert_eq!(look(&d, &m).unwrap().blend, Blend::Subtract);
        d.alpha = 0.5;
        assert_eq!(look(&d, &m).unwrap().blend, Blend::AlphaTest);
        // RENDER_BRIGHT alone: untextured additive without depth writes, not on TGA meshes.
        let l = look(&draw(RENDER_BRIGHT), &m).unwrap();
        assert_eq!((l.blend, l.untextured, l.depth_write), (Blend::Glow, true, Some(false)));
        assert!(look(&draw(RENDER_BRIGHT), &tga).is_none());
        // NODEPTH and WAVE.
        let l = look(&draw(RENDER_TEXTURE | RENDER_NODEPTH | RENDER_WAVE), &m).unwrap();
        assert!(l.no_depth_test && l.wave);
        // The blend mesh: additive, or subtractive with DARK; offset only on mesh index == BlendMesh.
        let mut d = draw(RENDER_TEXTURE | RENDER_DARK);
        d.blend_mesh = 0;
        d.blend_mesh_light = 0.5;
        d.blend_mesh_tex_coord = Vec2::new(0.1, 0.0);
        let l = look(&d, &m).unwrap();
        assert_eq!((l.blend, l.color, l.uv_offset), (Blend::Subtract, Vec3::splat(0.5), Vec2::new(0.1, 0.0)));
        d.blend_mesh = -2;
        assert_eq!(look(&d, &m).unwrap().uv_offset, Vec2::ZERO);
        // Colors are clamped like glColor.
        let mut d = draw(RENDER_TEXTURE);
        d.light = Vec3::splat(3.0);
        assert_eq!(look(&d, &m).unwrap().color, Vec3::ONE);
    }

    #[test]
    fn mesh_selection_and_scripts() {
        let mut d = draw(0);
        d.mesh = 1;
        assert!(look(&d, &mesh(0)).is_none());
        assert!(look(&d, &mesh(1)).is_some());
        // RenderMesh ignores HiddenMesh n and `_H`; -2 and BITMAP_HIDE always hide.
        d.hidden_mesh = 1;
        let h = MeshCtx { script: Some(TextureScript { hidden: true, ..Default::default() }), ..mesh(1) };
        assert!(look(&d, &h).is_some());
        d.mesh = -1;
        assert!(look(&d, &mesh(1)).is_none());
        d.hidden_mesh = -1;
        assert!(look(&d, &h).is_none());
        d.hidden_mesh = -2;
        assert!(look(&d, &mesh(0)).is_none());
        assert!(look(&draw(0), &MeshCtx { hide: true, ..mesh(0) }).is_none());
        // `_R` makes the mesh additive in RenderBody.
        let r = MeshCtx { script: Some(TextureScript { bright: true, ..Default::default() }), ..mesh(2) };
        assert_eq!(look(&draw(RENDER_TEXTURE), &r).unwrap().blend, Blend::Glow);
        let mut d = draw(RENDER_TEXTURE);
        d.mesh = 2;
        assert_eq!(look(&d, &r).unwrap().blend, Blend::Opaque);
    }

    #[test]
    fn historical_rules_without_flags() {
        let m = mesh(0);
        let mut d = draw(0);
        d.alpha = 0.5;
        assert_eq!(look(&d, &m).unwrap().blend, Blend::Blend3);
        d.bright = true;
        d.light = Vec3::splat(0.8);
        let l = look(&d, &m).unwrap();
        assert_eq!((l.blend, l.color, l.alpha), (Blend::Glow, Vec3::splat(0.4), 1.0));
        let mut d = draw(0);
        d.blend_mesh = -2;
        d.blend_mesh_tex_coord = Vec2::new(0.3, 0.0);
        assert_eq!(look(&d, &m).unwrap().uv_offset, Vec2::new(0.3, 0.0));
    }

    #[test]
    fn chrome_uvs_use_rotated_scaled_normals() {
        // Rotating 90° about X takes the model's +Y normal to world +Z.
        let rot = bmd::angle_matrix([std::f32::consts::FRAC_PI_2, 0.0, 0.0]);
        let uv = chrome_uvs(&[[0.0, 1.0, 0.0]], &rot, 1.0, ChromeMode::Chrome, 0.0, Vec2::ZERO)[0];
        assert!((uv[0] - 0.5).abs() < 1e-5 && uv[1].abs() < 1e-5, "{uv:?}");
        // The normal is `scale` long (ParentMatrix · BodyScale); METAL: n.z·0.5 + 0.2, n.y·0.5 + 0.5.
        let uv = chrome_uvs(&[[0.0, 0.0, 1.0]], &bmd::IDENTITY, 2.0, ChromeMode::Metal, 0.0, Vec2::new(0.1, 0.0))[0];
        assert!((uv[0] - 1.3).abs() < 1e-5 && (uv[1] - 0.5).abs() < 1e-5, "{uv:?}");
        // RENDER_CHROME at WorldTime 2500: Wave = 0.25.
        let uv = chrome_uvs(&[[0.0, 0.0, 0.0]], &bmd::IDENTITY, 1.0, ChromeMode::Chrome, 2500.0, Vec2::ZERO)[0];
        assert!((uv[0] - 0.25).abs() < 1e-5 && (uv[1] - 0.5).abs() < 1e-5, "{uv:?}");
    }

    #[test]
    fn wave_moves_along_normals() {
        let p = wave_positions(&[[1.0, 2.0, 3.0]; 2], &[[0.0, 0.0, 1.0]; 2], &[0, 2], 1000.0);
        let s0 = (1000.0f32 * 0.007).sin() * 28.0;
        let s1 = ((1000 + 2 * 931) as f32 * 0.007).sin() * 28.0;
        assert!((p[0][2] - (3.0 + s0)).abs() < 1e-3 && (p[1][2] - (3.0 + s1)).abs() < 1e-3);
        assert_eq!((p[0][0], p[1][1]), (1.0, 2.0));
    }

    #[test]
    fn corners_match_pose_with() {
        let v = |x: f32| bmd::Vertex { bone: 0, position: [x, 0.0, 0.0] };
        let m = bmd::Mesh {
            vertices: vec![v(0.0), v(1.0), v(2.0), v(3.0)],
            normals: vec![bmd::Normal { bone: 0, normal: [0.0, 0.0, 1.0] }],
            uvs: vec![[0.0, 0.0]],
            triangles: vec![
                bmd::Triangle { polygon: 3, vertex: [3, 1, 2, 0], normal: [0; 4], uv: [0; 4] },
                bmd::Triangle { polygon: 4, vertex: [0, 1, 2, 3], normal: [0; 4], uv: [0; 4] },
            ],
            texture_index: 0,
            texture: "a.jpg".into(),
        };
        let corners = corner_vertices(&m);
        assert_eq!(corners, vec![3, 1, 2, 0, 1, 2, 0, 2, 3]);
        let model = Bmd { version: 10, name: String::new(), meshes: vec![m], actions: vec![], bones: vec![] };
        let posed = model.pose_with(0, &[bmd::IDENTITY]);
        let xs: Vec<u32> = posed.positions.iter().map(|p| p[0] as u32).collect();
        assert_eq!(xs, corners);
    }
}
