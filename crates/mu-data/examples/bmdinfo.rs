//! Huesos, animaciones y caja (en bruto y posada) de un `.bmd` (depuración).
//!
//!   cargo run -p mu-data --example bmdinfo -- <carpeta> <archivo.bmd>

use std::path::PathBuf;

use mu_data::{bmd, fs};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(&args[0]);
    let m = bmd::read(&fs::read(&dir, &args[1]).unwrap()).unwrap();
    let frames: Vec<usize> = m.actions.iter().map(|a| a.frames).collect();
    println!("{} v{}: {} mallas, acciones {frames:?}", m.name, m.version, m.meshes.len());
    for (i, b) in m.bones.iter().enumerate() {
        let p = b.positions.first().and_then(|f| f.first());
        let r = b.rotations.first().and_then(|f| f.first());
        println!("  hueso {i} '{}' padre {} dummy {} pos {:?} rot {:?}", b.name, b.parent, b.dummy, p, r);
    }
    for (i, mesh) in m.meshes.iter().enumerate() {
        let bbox = |pts: &mut dyn Iterator<Item = [f32; 3]>| {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in pts {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            format!("{lo:.0?}..{hi:.0?}")
        };
        let mut bones: Vec<i16> = mesh.vertices.iter().map(|v| v.bone).collect();
        bones.sort();
        bones.dedup();
        println!(
            "  malla {i} '{}' {} vértices huesos {bones:?}\n    bruto {}\n    posado {}",
            mesh.texture,
            mesh.vertices.len(),
            bbox(&mut mesh.vertices.iter().map(|v| v.position)),
            bbox(&mut m.pose(i, 0, 0).positions.into_iter()),
        );
    }
}
