//! Objetos por encima del suelo: altura del objeto menos la del terreno en su sitio (depuración).
//!
//!   cargo run -p mu-data --example floating -- <world> [tipo_min] [tipo_max] [data]

use std::collections::BTreeMap;
use std::path::PathBuf;

use mu_data::{objects, terrain};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let world: u16 = args[0].parse().unwrap();
    let lo: i16 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(i16::MIN);
    let hi: i16 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(i16::MAX);
    let data = PathBuf::from(args.get(3).map(String::as_str).unwrap_or("../referencias/MuMain/src/bin/Data"));
    let list = objects::load(&data, world).unwrap();
    let t = terrain::load(&data, world).unwrap();
    let mut by_kind: BTreeMap<i16, Vec<(f32, [f32; 3], [f32; 3])>> = BTreeMap::new();
    for o in list.iter().filter(|o| (lo..=hi).contains(&o.kind)) {
        let ground = t.height_at(o.position[0] / 100.0, o.position[1] / 100.0);
        by_kind.entry(o.kind).or_default().push((o.position[2] - ground, o.position, o.angle));
    }
    // Huérfanos: más de 60 por encima del suelo sin ningún otro objeto (que no sea del mismo
    // rango) a menos de 2.5 casillas que llegue a su altura.
    let tops: Vec<([f32; 3], f32)> = list
        .iter()
        .filter(|o| !(lo..=hi).contains(&o.kind))
        .map(|o| (o.position, o.position[2] + 500.0 * o.scale))
        .collect();
    let mut orphans = 0;
    for o in list.iter().filter(|o| (lo..=hi).contains(&o.kind)) {
        let ground = t.height_at(o.position[0] / 100.0, o.position[1] / 100.0);
        if o.position[2] - ground < 60.0 {
            continue;
        }
        let near = tops.iter().any(|(p, top)| {
            let d = ((p[0] - o.position[0]).powi(2) + (p[1] - o.position[1]).powi(2)).sqrt();
            d < 250.0 && *top > o.position[2] - 50.0
        });
        if !near {
            orphans += 1;
            println!(
                "huérfano tipo {} +{:.0} en ({:.1},{:.1}) ang {:?}",
                o.kind,
                o.position[2] - ground,
                o.position[0] / 100.0,
                o.position[1] / 100.0,
                o.angle
            );
        }
    }
    println!("{orphans} huérfanos");
    for (k, mut v) in by_kind {
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        let n = v.len();
        let over = |d: f32| v.iter().filter(|e| e.0 > d).count();
        println!(
            "tipo {k:4} {:?} x{n}: min {:.0} mediana {:.0} max {:.0} | >30: {} >100: {}",
            objects::model_path(world, k).unwrap_or_default(),
            v[0].0,
            v[n / 2].0,
            v[n - 1].0,
            over(30.0),
            over(100.0)
        );
        for e in v.iter().rev().take(3) {
            println!(
                "    +{:.0} en ({:.1},{:.1}) ang {:?}",
                e.0,
                e.1[0] / 100.0,
                e.1[1] / 100.0,
                e.2
            );
        }
    }
}
