//! Objects near an object type or a tile (debugging).
//!
//!   cargo run -p mu-data --example near -- <world> <type|x,y> [radius_tiles=6] [data]

use std::collections::BTreeMap;
use std::path::PathBuf;

use mu_data::{objects, terrain};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let world: u16 = args[0].parse().unwrap();
    let radius: f32 = args.get(2).and_then(|r| r.parse().ok()).unwrap_or(6.0);
    let data = PathBuf::from(args.get(3).map(String::as_str).unwrap_or("../referencias/MuMain/src/bin/Data"));
    let list = objects::load(&data, world).unwrap();
    let center = if let Some((x, y)) = args[1].split_once(',') {
        [x.parse::<f32>().unwrap() * 100.0, y.parse::<f32>().unwrap() * 100.0]
    } else {
        let kind: i16 = args[1].parse().unwrap();
        let o = list.iter().find(|o| o.kind == kind).expect("tipo no encontrado");
        [o.position[0], o.position[1]]
    };
    println!("centro: casilla ({:.1}, {:.1})", center[0] / 100.0, center[1] / 100.0);
    let mut by_kind: BTreeMap<i16, Vec<String>> = BTreeMap::new();
    for o in &list {
        let d = ((o.position[0] - center[0]).powi(2) + (o.position[1] - center[1]).powi(2)).sqrt() / 100.0;
        if d <= radius {
            by_kind.entry(o.kind).or_default().push(format!(
                "({:.1},{:.1},z{:.0} esc {:.2} ang {:?})",
                o.position[0] / 100.0,
                o.position[1] / 100.0,
                o.position[2],
                o.scale,
                o.angle
            ));
        }
    }
    for (k, v) in by_kind {
        let rel = objects::model_path(world, k).unwrap_or_default();
        // Model box in its initial pose (MU units, unrotated).
        let bbox = (|| {
            let dir = data.join(rel.parent()?);
            let b = mu_data::bmd::read(&mu_data::fs::read(&dir, rel.file_name()?.to_str()?).ok()?).ok()?;
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for m in 0..b.meshes.len() {
                for p in b.pose(m, 0, 0).positions {
                    for i in 0..3 {
                        lo[i] = lo[i].min(p[i]);
                        hi[i] = hi[i].max(p[i]);
                    }
                }
            }
            Some(format!("caja {:.0?}..{:.0?}", lo, hi))
        })()
        .unwrap_or_default();
        println!("tipo {k:4} {rel:?} {bbox} x{}: {}", v.len(), v.join(" "));
    }
    if let Ok(t) = terrain::load(&data, world) {
        let (cx, cy) = ((center[0] / 100.0) as i32, (center[1] / 100.0) as i32);
        println!("capa1/capa2/alfa alrededor (filas y de arriba abajo):");
        for y in (cy - 4..=cy + 4).rev() {
            let row: Vec<String> = (cx - 4..=cx + 4)
                .map(|x| {
                    let i = (y as usize) * terrain::SIZE + x as usize;
                    format!("{:>2}/{:>3}/{:>3}", t.mapping.layer1[i], t.mapping.layer2[i], t.mapping.alpha[i])
                })
                .collect();
            println!("y{y:3}: {}", row.join(" "));
        }
    }
}
