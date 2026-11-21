//! Modelos 3D `.bmd` (objetos del mapa, personajes, monstruos, items).
//!
//! Estructura del archivo:
//! - `"BMD"` + versión (10 = sin cifrar, 12 = cifrado con `map_file_decrypt`)
//! - nombre, número de mallas, huesos y animaciones
//! - por cada malla: vértices, normales, coordenadas UV, triángulos y nombre de textura
//! - por cada animación: número de fotogramas
//! - por cada hueso: padre y, por animación y fotograma, posición + rotación
//!
//! Los vértices están en el espacio de su hueso, así que para dibujar el modelo
//! hay que aplicar la pose de un fotograma (`Bmd::pose`).

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
    /// 3 = triángulo, 4 = cuadrilátero.
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
    /// Nombre del archivo de textura tal como lo guarda el modelo (p. ej. `tree_01.jpg`).
    pub texture: String,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub frames: usize,
    /// Desplazamiento del modelo por fotograma (animaciones de caminar, etc.).
    pub lock_positions: Option<Vec<[f32; 3]>>,
}

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    /// `-1` = hueso raíz. Los huesos "dummy" no tienen datos.
    pub parent: i16,
    pub dummy: bool,
    /// `[animación][fotograma]`: posición relativa al padre.
    pub positions: Vec<Vec<[f32; 3]>>,
    /// `[animación][fotograma]`: rotación en radianes (x, y, z).
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
        12 => {
            let size = i32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
            let enc = buf.get(8..8 + size).ok_or_else(|| Error::Format("BMD cortado".into()))?;
            crypt::map_file_decrypt(enc)
        }
        15 => return Err(Error::Format("BMD v15 (cifrado LEA, Season 12+) no soportado".into())),
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

        // En disco cada estructura va alineada a 4 bytes (como el struct de C original).
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
            r.skip(4)?; // vértice asociado + relleno
            normals.push(Normal { bone, normal });
        }
        let mut uvs = Vec::with_capacity(nt);
        for _ in 0..nt {
            uvs.push([r.f32()?, r.f32()?]);
        }
        let mut triangles = Vec::with_capacity(ntri);
        for _ in 0..ntri {
            // 64 bytes: polígono(1) + relleno(1) + 3×[i16;4] + relleno(2) + lightmap(32) + índice(2) + relleno(2)
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

/// Matriz afín 3x4 (filas): rotación + traslación, como `float[3][4]` del original.
pub type Mat34 = [[f32; 4]; 3];

pub const IDENTITY: Mat34 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

/// Rotación de ángulos de Euler (radianes) igual que `AngleMatrix` del original:
/// primero X, luego Y, luego Z (R = Rz · Ry · Rx).
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

/// Malla lista para dibujar: triángulos sueltos (3 vértices cada uno), ya posados.
#[derive(Debug, Clone, Default)]
pub struct PosedMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
}

impl Bmd {
    /// Matriz de cada hueso en el fotograma `frame` de la animación `action`.
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
            // Los padres siempre van antes que los hijos en el archivo.
            out[i] = match usize::try_from(b.parent).ok().filter(|&pi| pi < i) {
                Some(pi) => concat(&out[pi], &local),
                None => local,
            };
        }
        out
    }

    /// Geometría de la malla `mesh` en la pose (animación, fotograma) indicada.
    pub fn pose(&self, mesh: usize, action: usize, frame: usize) -> PosedMesh {
        let bones = self.bone_matrices(action, frame);
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
        // Por si algún índice era inválido: dejamos solo triángulos completos.
        let n = out.positions.len() / 3 * 3;
        out.positions.truncate(n);
        out.normals.truncate(n);
        out.uvs.truncate(n);
        out
    }
}

/// Nombre del archivo de textura en disco: `.jpg` → `.OZJ`, `.tga` → `.OZT`.
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
