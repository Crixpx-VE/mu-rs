//! `.bmd` 3D models (map objects, characters, monsters, items).
//!
//! File layout:
//! - `"BMD"` + version (10 = plain, 12 = encrypted with `map_file_decrypt`)
//! - name, number of meshes, bones and animations
//! - per mesh: vertices, normals, UV coordinates, triangles and texture name
//! - per animation: number of frames
//! - per bone: parent and, per animation and frame, position + rotation
//!
//! Vertices are in their bone's space, so drawing the model requires
//! applying a frame's pose (`Bmd::pose`).

use crate::reader::Reader;
use crate::{Error, Result, crypt};

#[derive(Debug, Clone)]
pub struct Vertex {
    pub bone: i16,
    pub position: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct Normal {
    pub bone: i16,
    pub normal: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct Triangle {
    /// 3 = triangle, 4 = quad.
    pub polygon: u8,
    pub vertex: [i16; 4],
    pub normal: [i16; 4],
    pub uv: [i16; 4],
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub normals: Vec<Normal>,
    pub uvs: Vec<[f32; 2]>,
    pub triangles: Vec<Triangle>,
    pub texture_index: i16,
    /// Texture file name as stored by the model (e.g. `tree_01.jpg`).
    pub texture: String,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub frames: usize,
    /// Model displacement per frame (walk animations, etc.).
    pub lock_positions: Option<Vec<[f32; 3]>>,
}

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    /// `-1` = root bone. "Dummy" bones have no data.
    pub parent: i16,
    pub dummy: bool,
    /// `[animation][frame]`: position relative to the parent.
    pub positions: Vec<Vec<[f32; 3]>>,
    /// `[animation][frame]`: rotation in radians (x, y, z).
    pub rotations: Vec<Vec<[f32; 3]>>,
}

#[derive(Debug, Clone)]
pub struct Bmd {
    pub version: u8,
    pub name: String,
    pub meshes: Vec<Mesh>,
    pub actions: Vec<Action>,
    pub bones: Vec<Bone>,
}

pub fn read(buf: &[u8]) -> Result<Bmd> {
    if buf.len() < 8 || &buf[..3] != b"BMD" {
        return Err(Error::Format("no es un BMD".into()));
    }
    let version = buf[3];
    let data: Vec<u8> = match version {
        10 => buf[4..].to_vec(),
        12 | 15 => {
            let size = i32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
            let enc = buf.get(8..8 + size).ok_or_else(|| Error::Format("BMD cortado".into()))?;
            if version == 12 { crypt::map_file_decrypt(enc) } else { crypt::lea_decrypt(enc) }
        }
        v => return Err(Error::Format(format!("versión de BMD desconocida: {v}"))),
    };

    let mut r = Reader::new(&data);
    let name = r.string(32)?;
    let mesh_count = r.u16()? as usize;
    let bone_count = r.u16()? as usize;
    let action_count = r.u16()? as usize;

    let mut meshes = Vec::with_capacity(mesh_count);
    for _ in 0..mesh_count {
        let nv = r.i16()?.max(0) as usize;
        let nn = r.i16()?.max(0) as usize;
        let nt = r.i16()?.max(0) as usize;
        let ntri = r.i16()?.max(0) as usize;
        let texture_index = r.i16()?;

        // On disk each struct is 4-byte aligned (like the original C struct).
        let mut vertices = Vec::with_capacity(nv);
        for _ in 0..nv {
            let bone = r.i16()?;
            r.skip(2)?;
            vertices.push(Vertex { bone, position: r.vec3()? });
        }
        let mut normals = Vec::with_capacity(nn);
        for _ in 0..nn {
            let bone = r.i16()?;
            r.skip(2)?;
            let normal = r.vec3()?;
            r.skip(4)?; // associated vertex + padding
            normals.push(Normal { bone, normal });
        }
        let mut uvs = Vec::with_capacity(nt);
        for _ in 0..nt {
            uvs.push([r.f32()?, r.f32()?]);
        }
        let mut triangles = Vec::with_capacity(ntri);
        for _ in 0..ntri {
            // 64 bytes: polygon(1) + padding(1) + 3×[i16;4] + padding(2) + lightmap(32) + index(2) + padding(2)
            let polygon = r.u8()?;
            r.skip(1)?;
            let mut idx = [[0i16; 4]; 3];
            for set in idx.iter_mut() {
                for v in set.iter_mut() {
                    *v = r.i16()?;
                }
            }
            r.skip(2 + 32 + 2 + 2)?;
            triangles.push(Triangle { polygon, vertex: idx[0], normal: idx[1], uv: idx[2] });
        }
        let texture = r.string(32)?;
        meshes.push(Mesh { vertices, normals, uvs, triangles, texture_index, texture });
    }

    let mut actions = Vec::with_capacity(action_count);
    for _ in 0..action_count {
        let frames = r.i16()?.max(0) as usize;
        let lock = r.u8()? != 0;
        let lock_positions = if lock {
            Some((0..frames).map(|_| r.vec3()).collect::<Result<Vec<_>>>()?)
        } else {
            None
        };
        actions.push(Action { frames, lock_positions });
    }

    let mut bones = Vec::with_capacity(bone_count);
    for _ in 0..bone_count {
        if r.u8()? != 0 {
            bones.push(Bone {
                name: String::new(),
                parent: -1,
                dummy: true,
                positions: vec![],
                rotations: vec![],
            });
            continue;
        }
        let name = r.string(32)?;
        let parent = r.i16()?;
        let mut positions = Vec::with_capacity(actions.len());
        let mut rotations = Vec::with_capacity(actions.len());
        for a in &actions {
            positions.push((0..a.frames).map(|_| r.vec3()).collect::<Result<Vec<_>>>()?);
            rotations.push((0..a.frames).map(|_| r.vec3()).collect::<Result<Vec<_>>>()?);
        }
        bones.push(Bone { name, parent, dummy: false, positions, rotations });
    }

    Ok(Bmd { version, name, meshes, actions, bones })
}

/// 3x4 affine matrix (rows): rotation + translation, like the original `float[3][4]`.
pub type Mat34 = [[f32; 4]; 3];

pub const IDENTITY: Mat34 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

/// Euler-angle rotation (radians) matching the original `AngleMatrix`:
/// X first, then Y, then Z (R = Rz · Ry · Rx).
pub fn angle_matrix(a: [f32; 3]) -> Mat34 {
    let (sr, cr) = a[0].sin_cos();
    let (sp, cp) = a[1].sin_cos();
    let (sy, cy) = a[2].sin_cos();
    [
        [cp * cy, sr * sp * cy - cr * sy, cr * sp * cy + sr * sy, 0.0],
        [cp * sy, sr * sp * sy + cr * cy, cr * sp * sy - sr * cy, 0.0],
        [-sp, sr * cp, cr * cp, 0.0],
    ]
}

pub fn concat(a: &Mat34, b: &Mat34) -> Mat34 {
    let mut o = [[0.0; 4]; 3];
    for i in 0..3 {
        for j in 0..4 {
            o[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
        o[i][3] += a[i][3];
    }
    o
}

pub fn transform_point(m: &Mat34, p: [f32; 3]) -> [f32; 3] {
    let f = |r: usize| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3];
    [f(0), f(1), f(2)]
}

pub fn rotate_vector(m: &Mat34, v: [f32; 3]) -> [f32; 3] {
    let f = |r: usize| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2];
    [f(0), f(1), f(2)]
}

/// Quaternion `[x, y, z, w]` equivalent to [`angle_matrix`] (original `AngleQuaternion`).
pub fn angle_quaternion(a: [f32; 3]) -> [f32; 4] {
    let (sx, cx) = (a[0] * 0.5).sin_cos();
    let (sy, cy) = (a[1] * 0.5).sin_cos();
    let (sz, cz) = (a[2] * 0.5).sin_cos();
    [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ]
}

/// Spherical interpolation between two rotations (along the short path).
pub fn quat_slerp(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if dot < 0.0 {
        b = b.map(|v| -v);
        dot = -dot;
    }
    let (wa, wb) = if dot > 0.9995 {
        (1.0 - t, t)
    } else {
        let theta = dot.acos();
        let s = theta.sin();
        (((1.0 - t) * theta).sin() / s, (t * theta).sin() / s)
    };
    let q = [0, 1, 2, 3].map(|i| a[i] * wa + b[i] * wb);
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    q.map(|v| v / l)
}

pub fn quat_matrix(q: [f32; 4]) -> Mat34 {
    let [x, y, z, w] = q;
    [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - w * z), 2.0 * (x * z + w * y), 0.0],
        [2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x), 0.0],
        [2.0 * (x * z - w * y), 2.0 * (y * z + w * x), 1.0 - 2.0 * (x * x + y * y), 0.0],
    ]
}

/// Mesh ready to draw: loose triangles (3 vertices each), already posed.
#[derive(Debug, Clone, Default)]
pub struct PosedMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
}

impl Bmd {
    /// Matrix of each bone at frame `frame` of animation `action`.
    pub fn bone_matrices(&self, action: usize, frame: usize) -> Vec<Mat34> {
        let mut out: Vec<Mat34> = vec![IDENTITY; self.bones.len()];
        for (i, b) in self.bones.iter().enumerate() {
            if b.dummy {
                continue;
            }
            let (Some(pos), Some(rot)) = (b.positions.get(action), b.rotations.get(action)) else {
                continue;
            };
            let (Some(&p), Some(&r)) = (pos.get(frame), rot.get(frame)) else { continue };
            let mut local = angle_matrix(r);
            local[0][3] = p[0];
            local[1][3] = p[1];
            local[2][3] = p[2];
            // Parents always come before children in the file.
            out[i] = match usize::try_from(b.parent).ok().filter(|&pi| pi < i) {
                Some(pi) => concat(&out[pi], &local),
                None => local,
            };
        }
        out
    }

    /// Like [`Bmd::bone_matrices`] but at an in-between instant: `frame` 2.25 blends
    /// frame 2 (75 %) with frame 3 (25 %). At the end it wraps to the first one.
    ///
    /// In animations with `lock_positions` (walk, run...) the root bone stays at the
    /// X/Y of the first frame: the game applies the displacement, not the animation.
    /// Also their last frame repeats the first, so the loop has one frame less.
    pub fn bone_matrices_at(&self, action: usize, frame: f32) -> Vec<Mat34> {
        let Some(act) = self.actions.get(action).filter(|a| a.frames > 0) else {
            return self.bone_matrices(action, 0);
        };
        let locked = act.lock_positions.is_some();
        let frames = if locked { (act.frames - 1).max(1) } else { act.frames };
        let f = frame.rem_euclid(frames as f32);
        let f0 = f.floor() as usize % frames;
        let f1 = (f0 + 1) % frames;
        let t = f - f.floor();

        let mut out: Vec<Mat34> = vec![IDENTITY; self.bones.len()];
        for (i, b) in self.bones.iter().enumerate() {
            if b.dummy {
                continue;
            }
            let (Some(pos), Some(rot)) = (b.positions.get(action), b.rotations.get(action)) else {
                continue;
            };
            let (Some(p0), Some(p1), Some(r0), Some(r1)) = (pos.get(f0), pos.get(f1), rot.get(f0), rot.get(f1))
            else {
                continue;
            };
            let q = quat_slerp(angle_quaternion(*r0), angle_quaternion(*r1), t);
            let mut local = quat_matrix(q);
            for k in 0..3 {
                local[k][3] = p0[k] + (p1[k] - p0[k]) * t;
            }
            if i == 0 && locked {
                local[0][3] = pos[0][0];
                local[1][3] = pos[0][1];
            }
            out[i] = match usize::try_from(b.parent).ok().filter(|&pi| pi < i) {
                Some(pi) => concat(&out[pi], &local),
                None => local,
            };
        }
        out
    }

    /// Geometry of mesh `mesh` in the given pose (animation, frame).
    pub fn pose(&self, mesh: usize, action: usize, frame: usize) -> PosedMesh {
        self.pose_with(mesh, &self.bone_matrices(action, frame))
    }

    /// Geometry of mesh `mesh` with the given bones. Used for armor pieces,
    /// which have no animations of their own and use the `player.bmd` skeleton.
    pub fn pose_with(&self, mesh: usize, bones: &[Mat34]) -> PosedMesh {
        let m = &self.meshes[mesh];
        let bone = |b: i16| bones.get(b as usize).unwrap_or(&IDENTITY);
        let mut out = PosedMesh::default();

        for t in &m.triangles {
            let corners: &[usize] = if t.polygon == 4 { &[0, 1, 2, 0, 2, 3] } else { &[0, 1, 2] };
            for &c in corners {
                let (Some(v), Some(n), Some(uv)) = (
                    m.vertices.get(t.vertex[c] as usize),
                    m.normals.get(t.normal[c] as usize),
                    m.uvs.get(t.uv[c] as usize),
                ) else {
                    continue;
                };
                out.positions.push(transform_point(bone(v.bone), v.position));
                out.normals.push(rotate_vector(bone(n.bone), n.normal));
                out.uvs.push(*uv);
            }
        }
        // In case some index was invalid: keep only complete triangles.
        let n = out.positions.len() / 3 * 3;
        out.positions.truncate(n);
        out.normals.truncate(n);
        out.uvs.truncate(n);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuaternion_y_matriz_coinciden() {
        for a in [[0.3, -1.2, 2.0], [1.5, 0.2, -0.7], [0.0, 0.0, 3.1], [-2.0, 1.0, 0.5]] {
            let m1 = angle_matrix(a);
            let m2 = quat_matrix(angle_quaternion(a));
            for r in 0..3 {
                for c in 0..3 {
                    assert!((m1[r][c] - m2[r][c]).abs() < 1e-5, "{a:?} [{r}][{c}] {} vs {}", m1[r][c], m2[r][c]);
                }
            }
        }
    }

    /// BMD v15 (LEA) from an S20 client extracted to `MU_S20_DIR`: the same model as the S6 v10.
    #[test]
    fn bmd_lea_v15() {
        let Ok(dir) = std::env::var("MU_S20_DIR") else { return };
        let dir = std::path::Path::new(&dir);
        let show = |name: &str| {
            let m = read(&std::fs::read(dir.join(name)).unwrap()).expect(name);
            let info: Vec<_> = m.meshes.iter().map(|x| (x.vertices.len(), x.triangles.len(), x.texture.clone())).collect();
            eprintln!("{name}: v{} '{}' {} huesos {} acciones mallas {info:?}", m.version, m.name, m.bones.len(), m.actions.len());
            m
        };
        let s20 = show("Object40.bmd");
        if let Ok(s6) = std::env::var("MU_S6_OBJECT40") {
            let s6 = read(&std::fs::read(s6).unwrap()).unwrap();
            assert_eq!(s20.meshes.len(), s6.meshes.len());
            assert_eq!(s20.meshes[0].vertices.len(), s6.meshes[0].vertices.len());
        }
        if dir.join("Object41.bmd").exists() {
            show("Object41.bmd");
        }
    }

    #[test]
    fn interpolar_en_fotograma_exacto_da_la_pose_exacta() {
        let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data/Player");
        let Ok(buf) = crate::fs::read(&data, "player.bmd") else { return };
        let m = read(&buf).expect("player.bmd");
        eprintln!("player.bmd: {} huesos, {} animaciones", m.bones.len(), m.actions.len());
        // Without root locking (animation 0) the exact frame must match.
        let a = m.bone_matrices(0, 0);
        let b = m.bone_matrices_at(0, 0.0);
        for (x, y) in a.iter().zip(&b) {
            for r in 0..3 {
                for c in 0..4 {
                    assert!((x[r][c] - y[r][c]).abs() < 1e-3);
                }
            }
        }

        // Idle and walking: the armor must end up human-sized (~2 m = 200 units).
        let armor = read(&crate::fs::read(&data, "ArmorClass02.bmd").unwrap()).unwrap();
        for action in [1usize, 15] {
            eprintln!("acción {action}: {} fotogramas, lock={}", m.actions[action].frames, m.actions[action].lock_positions.is_some());
            for i in 0..40 {
                let bones = m.bone_matrices_at(action, i as f32 * 0.37);
                for mesh in 0..armor.meshes.len() {
                    for p in armor.pose_with(mesh, &bones).positions {
                        assert!(p.iter().all(|v| v.is_finite() && v.abs() < 400.0), "acción {action} t {i}: {p:?}");
                    }
                }
            }
        }
    }
}

/// Texture script (`TextureScript.h`): letters after the first `_` of a mesh's texture name
/// (`ju01_R.jpg`, `test12_H.jpg`...) that change how that mesh is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextureScript {
    /// `R` (`getBright`): in `RenderBody` the mesh is its own `BlendMesh` (adds its color).
    pub bright: bool,
    /// `H` (`getHiddenMesh`): `RenderBody` does not draw it.
    pub hidden: bool,
    /// `S` (`getStreamMesh`): its texture scrolls with `BlendMeshTexCoordU/V` (like `StreamMesh`,
    /// but still lit).
    pub stream: bool,
    /// `N` (`getNoneBlendMesh`): the chrome passes (`RENDER_CHROME*`, `METAL`, `OIL`) do not draw it.
    pub none_blend: bool,
}

/// `TextureScriptParsing::parsingTScript` (`TextureScript.cpp:16`) with the model's texture name
/// (the 32 bytes of `Textures[i].FileName`).
///
/// From the first `_` to the first `.`, at most 4 letters after the `_` are checked: each `R`, `H`,
/// `S` or `N` enables its option; any other (`tree_01`, `_RX`...) cancels the whole script (returns
/// `None` even if options were already set: `m_bBeScript = false`). Case-sensitive. The original's
/// `'DC'` and `'DT'` cases are two-letter constants that never match a single letter: no effect.
pub fn texture_script(name: &str) -> Option<TextureScript> {
    let bytes = name.as_bytes();
    let bytes = &bytes[..bytes.len().min(32)];
    let bytes = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
    let start = bytes.iter().position(|&b| b == b'_')?;
    // `strtok(strTokenFile, ".")`: starts at the `_` (which is not `.`) and ends at the first `.`.
    let token = &bytes[start..];
    let token = &token[..token.iter().position(|&b| b == b'.').unwrap_or(token.len())];
    let length = token.len().min(5);
    let mut script = TextureScript::default();
    let mut be_script = false;
    for &c in &token[1..length] {
        match c {
            b'R' => script.bright = true,
            b'H' => script.hidden = true,
            b'S' => script.stream = true,
            b'N' => script.none_blend = true,
            _ => return None,
        }
        be_script = true;
    }
    be_script.then_some(script)
}

#[cfg(test)]
mod script_tests {
    use super::*;

    #[test]
    fn guiones_de_textura() {
        let s = |bright, hidden, stream, none_blend| Some(TextureScript { bright, hidden, stream, none_blend });
        assert_eq!(texture_script("ju01_R.jpg"), s(true, false, false, false));
        assert_eq!(texture_script("test12_H.jpg"), s(false, true, false, false));
        assert_eq!(texture_script("water_S.tga"), s(false, false, true, false));
        assert_eq!(texture_script("rock_N.jpg"), s(false, false, false, true));
        assert_eq!(texture_script("glow_RS.jpg"), s(true, false, true, false));
        // Only 4 letters after the `_` are checked.
        assert_eq!(texture_script("x_RHSNQ.jpg"), s(true, true, true, true));
        // Any other letter cancels the script, even if a valid one came before.
        assert_eq!(texture_script("tree_01.jpg"), None);
        assert_eq!(texture_script("bridge_shadow01.tga"), None);
        assert_eq!(texture_script("a_RX.jpg"), None);
        assert_eq!(texture_script("a_r.jpg"), None);
        // Only the first `_` counts.
        assert_eq!(texture_script("tile_wood_R.jpg"), None);
        // No letters after the `_`, or no `_`.
        assert_eq!(texture_script("a_.jpg"), None);
        assert_eq!(texture_script("plain.jpg"), None);
        // No extension: up to the end.
        assert_eq!(texture_script("Kinglight_R"), s(true, false, false, false));
    }
}

/// Texture file name on disk: `.jpg` → `.OZJ`, `.tga` → `.OZT`.
pub fn texture_file_name(texture: &str) -> String {
    let file = texture.rsplit(['\\', '/']).next().unwrap_or(texture);
    match file.rsplit_once('.') {
        Some((stem, ext)) => match ext.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => format!("{stem}.OZJ"),
            "tga" => format!("{stem}.OZT"),
            "bmp" => format!("{stem}.OZB"),
            _ => file.to_string(),
        },
        None => file.to_string(),
    }
}
