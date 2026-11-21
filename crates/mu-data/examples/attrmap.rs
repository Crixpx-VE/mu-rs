//! Mapa de atributos de una zona (depuración): `.` se puede andar, `#` no, `~` sin suelo.
//!
//!   cargo run -p mu-data --example attrmap -- <world> x0 y0 x1 y1 [data]

use std::path::PathBuf;

use mu_data::terrain::{self, attr};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let world: u16 = a[0].parse().unwrap();
    let n = |i: usize| a[i].parse::<usize>().unwrap();
    let (x0, y0, x1, y1) = (n(1), n(2), n(3), n(4));
    let data = PathBuf::from(a.get(5).map(String::as_str).unwrap_or("../referencias/MuMain/src/bin/Data"));
    let t = terrain::load(&data, world).unwrap();
    // De arriba (y grande) a abajo, como se vería un plano.
    for y in (y0..=y1).rev() {
        let row: String = (x0..=x1)
            .map(|x| {
                let v = t.attribute(x, y);
                if v & attr::NO_GROUND != 0 {
                    '~'
                } else if v & attr::NO_MOVE != 0 {
                    '#'
                } else {
                    '.'
                }
            })
            .collect();
        println!("{y:3} {row}");
    }
    let header: String = (x0..=x1).map(|x| char::from(b'0' + (x % 10) as u8)).collect();
    println!("    {header}");
}
