//! Monstruos y NPC del mundo: dónde aparecen (`datos/MonsterSetBase.txt`, ver `tools/gen_spawns.py`), sus
//! datos (`datos/Monster.txt`) y cómo pasean.
//!
//! - NPC (sección 0 de `MonsterSetBase`): en su casilla, mirando a su dirección, quietos.
//! - Zonas (sección 1): `Count` monstruos en casillas al azar de la zona que se pueden pisar y no son
//!   zona segura.
//! - Sueltos (sección 2): en su casilla.
//!
//! Pasear es lo de `BasicMonsterIntelligence.TickWithoutTargetAsync` de OpenMU: cada `AttackSpeed`
//! milisegundos, si algún jugador lo ve y no está andando, va a una casilla al azar a como mucho
//! `MoveRange` en cada eje (`Monster.RandomMoveAsync`), con un paso cada `MoveSpeed` milisegundos (por la
//! distancia del paso, `Monster.GetStepDelay`) y sin pisar la zona segura (`CanWalkOnSafezone`). Como
//! mucho 15 pasos por paseo (los que caben en el paquete). Además no sale de su zona (las sueltas: su
//! `Range` o, si es 0, su `MoveRange` alrededor de su casilla); OpenMU no lo limita y con el tiempo los
//! monstruos se irían repartiendo por todo el mapa.
//!
//! Aún no: perseguir y atacar (`ViewRange`, `AttackRange`), morir y reaparecer (`RegenTime`).

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::time::{Duration, Instant};

use mu_data::terrain::{self, attr};
use mu_protocol::game::{self, ScopeMonster, ServerMessage};

/// Ids de objeto de los monstruos: de 1 a esto (los jugadores van por encima).
pub const MAX_MONSTERS: usize = 0x1FFF;

/// Pasos como mucho de un paseo (`MAX_WALK_STEPS`).
const MAX_STEPS: usize = game::MAX_WALK_STEPS;

/// Una fila de `Monster.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub number: u16,
    pub name: String,
    pub level: u16,
    pub move_range: u8,
    pub view_range: u8,
    /// Milisegundos por casilla.
    pub move_speed: u32,
    /// Milisegundos entre una decisión y la siguiente (`AttackDelay` en OpenMU).
    pub attack_speed: u32,
}

/// `Monster.txt`: `Index Rate "Name" Level MaxLife MaxMana DamageMin DamageMax Defense MagicDefense
/// AttackRate DefenseRate MoveRange AttackType AttackRange ViewRange MoveSpeed AttackSpeed RegenTime ...`.
pub fn parse_monster_txt(text: &str) -> HashMap<u16, Info> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        let (Some(a), Some(b)) = (line.find('"'), line.rfind('"')) else { continue };
        if a == b {
            continue;
        }
        let head: Vec<&str> = line[..a].split_whitespace().collect();
        let nums: Vec<i64> = line[b + 1..].split_whitespace().filter_map(|w| w.parse().ok()).collect();
        let Some(number) = head.first().and_then(|w| w.parse::<u16>().ok()) else { continue };
        if nums.len() < 16 {
            continue;
        }
        out.insert(
            number,
            Info {
                number,
                name: line[a + 1..b].to_string(),
                level: nums[0] as u16,
                move_range: nums[9].clamp(0, 255) as u8,
                view_range: nums[12].clamp(0, 255) as u8,
                move_speed: nums[13].max(0) as u32,
                attack_speed: nums[14].max(0) as u32,
            },
        );
    }
    out
}

/// Una fila de `MonsterSetBase.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnRow {
    /// 0 NPC, 1 zona, 2 suelto.
    pub section: u8,
    pub number: u16,
    pub map: u8,
    pub range: u8,
    pub x1: u8,
    pub y1: u8,
    pub x2: u8,
    pub y2: u8,
    /// 0..7, o -1 al azar.
    pub dir: i8,
    pub count: u16,
}

/// Secciones 0 (NPC), 1 (zonas) y 2 (sueltos); las de invasiones y eventos se saltan.
pub fn parse_setbase(text: &str) -> Vec<SpawnRow> {
    let mut rows = Vec::new();
    let mut section: Option<u8> = None;
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case("end") {
            section = None;
            continue;
        }
        let v: Vec<i32> = match line.split_whitespace().map(str::parse).collect() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(s) = section else {
            if v.len() == 1 {
                section = Some(v[0].clamp(0, 255) as u8);
            }
            continue;
        };
        let b = |i: usize| v[i].clamp(0, 255) as u8;
        match s {
            0 | 2 if v.len() >= 6 => rows.push(SpawnRow {
                section: s,
                number: v[0] as u16,
                map: b(1),
                range: b(2),
                x1: b(3),
                y1: b(4),
                x2: b(3),
                y2: b(4),
                dir: v[5].clamp(-1, 7) as i8,
                count: 1,
            }),
            1 if v.len() >= 9 => rows.push(SpawnRow {
                section: 1,
                number: v[0] as u16,
                map: b(1),
                range: b(2),
                x1: b(3).min(b(5)),
                y1: b(4).min(b(6)),
                x2: b(3).max(b(5)),
                y2: b(4).max(b(6)),
                dir: v[7].clamp(-1, 7) as i8,
                count: v[8].max(0) as u16,
            }),
            _ => {}
        }
    }
    rows
}

/// Un monstruo o NPC en el mundo.
pub struct Mob {
    pub id: u16,
    pub number: u16,
    pub map: u8,
    pub x: u8,
    pub y: u8,
    pub dir: u8,
    /// Zona por la que puede pasear (incluida): `(x1, y1, x2, y2)`.
    home: (u8, u8, u8, u8),
    /// NPC y monstruos con `MoveRange` 0 no se mueven.
    walks: bool,
    move_range: u8,
    move_speed: u32,
    attack_speed: u32,
    path: VecDeque<(u8, u8)>,
    next_step: Instant,
    next_think: Instant,
}

impl Mob {
    /// Cómo lo ve un jugador que lo tiene delante ahora.
    pub fn scope(&self) -> ScopeMonster {
        let (tx, ty) = self.path.back().copied().unwrap_or((self.x, self.y));
        ScopeMonster { id: self.id, number: self.number, x: self.x, y: self.y, target_x: tx, target_y: ty, rotation: self.dir }
    }

    /// Cuánto tarda en dar el paso a `to` (`MoveSpeed` por la distancia, `Monster.GetStepDelay`).
    fn step_delay(&self, to: (u8, u8)) -> Duration {
        let diagonal = to.0 != self.x && to.1 != self.y;
        let ms = self.move_speed as f64 * if diagonal { std::f64::consts::SQRT_2 } else { 1.0 };
        Duration::from_micros((ms * 1000.0) as u64)
    }
}

/// Un paseo que empieza: lo tienen que ver los que ven al monstruo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Walked {
    pub id: u16,
    pub message: ServerMessage,
}

#[derive(Default)]
pub struct Mobs {
    /// Por id: `list[id - 1]`.
    pub list: Vec<Mob>,
    terrains: HashMap<u8, Grid>,
    rng: Rng,
}

impl Mobs {
    /// Los de `datos/` con el terreno del cliente en `data` (`EncTerrainN.att`).
    pub fn load(data: &Path) -> Self {
        let infos = parse_monster_txt(include_str!("../datos/Monster.txt"));
        let rows = parse_setbase(include_str!("../datos/MonsterSetBase.txt"));
        let mut terrains = HashMap::new();
        let mut maps: Vec<u8> = rows.iter().map(|r| r.map).collect();
        maps.sort_unstable();
        maps.dedup();
        for map in maps {
            match terrain::load_attributes(data, map as u16 + 1).map(Grid) {
                Ok(t) => {
                    terrains.insert(map, t);
                }
                Err(e) => eprintln!("[monstruos] sin terreno del mapa {map} ({e}): sus monstruos no salen"),
            }
        }
        let mobs = Self::build(&rows, &infos, terrains, Instant::now(), Rng::from_time());
        let mut per_map: Vec<(u8, usize)> = Vec::new();
        for m in &mobs.list {
            match per_map.last_mut() {
                Some((map, n)) if *map == m.map => *n += 1,
                _ => per_map.push((m.map, 1)),
            }
        }
        eprintln!("[monstruos] {} en {} mapas: {per_map:?}", mobs.list.len(), per_map.len());
        mobs
    }

    fn build(rows: &[SpawnRow], infos: &HashMap<u16, Info>, terrains: HashMap<u8, Grid>, now: Instant, mut rng: Rng) -> Self {
        let mut list = Vec::new();
        let mut missing = Vec::new();
        for row in rows {
            let Some(t) = terrains.get(&row.map) else { continue };
            let Some(info) = infos.get(&row.number) else {
                missing.push(row.number);
                continue;
            };
            for _ in 0..row.count {
                if list.len() >= MAX_MONSTERS {
                    eprintln!("[monstruos] más de {MAX_MONSTERS}: el resto no sale");
                    break;
                }
                let (x, y) = if row.section == 1 {
                    match free_tile(t, (row.x1, row.y1, row.x2, row.y2), &mut rng) {
                        Some(p) => p,
                        None => continue,
                    }
                } else {
                    (row.x1, row.y1)
                };
                // La zona por la que pasea: la suya; si es una casilla, su `Range` (o su `MoveRange`) alrededor.
                let around = if row.range > 0 { row.range } else { info.move_range };
                let home = if (row.x1, row.y1) == (row.x2, row.y2) {
                    (row.x1.saturating_sub(around), row.y1.saturating_sub(around), row.x2.saturating_add(around), row.y2.saturating_add(around))
                } else {
                    (row.x1, row.y1, row.x2, row.y2)
                };
                let dir = if row.dir < 0 { (rng.next() % 8) as u8 } else { row.dir as u8 };
                // Que no decidan todos a la vez.
                let first = Duration::from_millis(info.attack_speed as u64 + rng.next() % 1000);
                list.push(Mob {
                    id: list.len() as u16 + 1,
                    number: row.number,
                    map: row.map,
                    x,
                    y,
                    dir,
                    home,
                    walks: row.section != 0 && info.move_range > 0 && info.move_speed > 0,
                    move_range: info.move_range,
                    move_speed: info.move_speed,
                    attack_speed: info.attack_speed.max(100),
                    path: VecDeque::new(),
                    next_step: now,
                    next_think: now + first,
                });
            }
        }
        missing.sort_unstable();
        missing.dedup();
        if !missing.is_empty() {
            eprintln!("[monstruos] sin datos en Monster.txt (no salen): {missing:?}");
        }
        Self { list, terrains, rng }
    }

    pub fn get(&self, id: u16) -> Option<&Mob> {
        self.list.get((id as usize).checked_sub(1)?)
    }

    /// Avanza hasta `now`: los que andan dan sus pasos y los que ve alguien (`observed(id)`) y están
    /// quietos deciden si pasean. Devuelve los paseos que empiezan.
    pub fn tick(&mut self, now: Instant, observed: impl Fn(u16) -> bool) -> Vec<Walked> {
        let mut out = Vec::new();
        let Self { list, terrains, rng } = self;
        for m in list.iter_mut() {
            while let Some(&next) = m.path.front() {
                if now < m.next_step {
                    break;
                }
                if let Some(d) = game::direction_between((m.x, m.y), next) {
                    m.dir = d;
                }
                (m.x, m.y) = next;
                m.path.pop_front();
                if let Some(&after) = m.path.front() {
                    m.next_step += m.step_delay(after);
                }
            }
            if !m.walks || now < m.next_think {
                continue;
            }
            m.next_think = now + Duration::from_millis(m.attack_speed as u64);
            if !m.path.is_empty() || !observed(m.id) {
                continue;
            }
            let Some(t) = terrains.get(&m.map) else { continue };
            let r = m.move_range as i32;
            let pick = |v: u8, lo: u8, hi: u8, rng: &mut Rng| {
                let d = (rng.next() % (2 * r as u64 + 1)) as i32 - r;
                (v as i32 + d).clamp(lo as i32, hi as i32) as u8
            };
            let target = (pick(m.x, m.home.0, m.home.2, rng), pick(m.y, m.home.1, m.home.3, rng));
            if target == (m.x, m.y) || !monster_can_stand(t, target.0, target.1) {
                continue;
            }
            let Some(steps) = short_path(t, (m.x, m.y), target, m.home, m.move_range as usize + 2) else { continue };
            let steps: Vec<(u8, u8)> = steps.into_iter().take(MAX_STEPS).collect();
            let Some(&last) = steps.last() else { continue };
            let mut from = (m.x, m.y);
            let mut directions = Vec::with_capacity(steps.len());
            for &s in &steps {
                directions.push(game::direction_between(from, s).unwrap_or(0));
                from = s;
            }
            m.next_step = now + m.step_delay(steps[0]);
            m.path = steps.into_iter().collect();
            out.push(Walked {
                id: m.id,
                message: ServerMessage::ObjectWalked {
                    id: m.id,
                    target_x: last.0,
                    target_y: last.1,
                    rotation: *directions.last().unwrap_or(&m.dir),
                    directions,
                },
            });
        }
        out
    }
}

/// Las banderas de las casillas de un mapa (`EncTerrainN.att`, ver [`attr`]).
pub struct Grid(Vec<u16>);

impl Default for Grid {
    /// Todo pisable.
    fn default() -> Self {
        Self(vec![0; terrain::SIZE * terrain::SIZE])
    }
}

impl Grid {
    fn attribute(&self, x: usize, y: usize) -> u16 {
        if x >= terrain::SIZE || y >= terrain::SIZE {
            return attr::NO_MOVE;
        }
        self.0[y * terrain::SIZE + x]
    }

    fn walkable(&self, x: usize, y: usize) -> bool {
        self.attribute(x, y) & (attr::NO_MOVE | attr::NO_GROUND) == 0
    }

    #[cfg(test)]
    fn set_attribute(&mut self, x: usize, y: usize, a: u16) {
        self.0[y * terrain::SIZE + x] = a;
    }
}

/// Un monstruo puede estar en una casilla que se pisa y no es zona segura.
fn monster_can_stand(t: &Grid, x: u8, y: u8) -> bool {
    t.walkable(x as usize, y as usize) && t.attribute(x as usize, y as usize) & attr::SAFE_ZONE == 0
}

/// Una casilla al azar de la zona en la que puede estar un monstruo.
fn free_tile(t: &Grid, (x1, y1, x2, y2): (u8, u8, u8, u8), rng: &mut Rng) -> Option<(u8, u8)> {
    let (w, h) = (x2 as u64 - x1 as u64 + 1, y2 as u64 - y1 as u64 + 1);
    for _ in 0..100 {
        let (x, y) = (x1 + (rng.next() % w) as u8, y1 + (rng.next() % h) as u8);
        if monster_can_stand(t, x, y) {
            return Some((x, y));
        }
    }
    None
}

/// Camino más corto en pasos (sin la casilla de salida) de `from` a `to` sin salir de `home` ni de
/// `reach` casillas alrededor de `from`, por casillas donde puede estar un monstruo.
fn short_path(t: &Grid, from: (u8, u8), to: (u8, u8), home: (u8, u8, u8, u8), reach: usize) -> Option<Vec<(u8, u8)>> {
    let side = 2 * reach + 1;
    let ox = from.0 as i32 - reach as i32;
    let oy = from.1 as i32 - reach as i32;
    let inside = |x: i32, y: i32| {
        (0..side as i32).contains(&(x - ox))
            && (0..side as i32).contains(&(y - oy))
            && (home.0 as i32..=home.2 as i32).contains(&x)
            && (home.1 as i32..=home.3 as i32).contains(&y)
            && x < terrain::SIZE as i32
            && y < terrain::SIZE as i32
    };
    let idx = |x: i32, y: i32| ((y - oy) as usize) * side + (x - ox) as usize;
    let mut came = vec![u32::MAX; side * side];
    let mut queue = VecDeque::new();
    came[idx(from.0 as i32, from.1 as i32)] = idx(from.0 as i32, from.1 as i32) as u32;
    queue.push_back((from.0 as i32, from.1 as i32));
    while let Some((x, y)) = queue.pop_front() {
        if (x, y) == (to.0 as i32, to.1 as i32) {
            let mut path = Vec::new();
            let mut cur = idx(x, y);
            let start = idx(from.0 as i32, from.1 as i32);
            while cur != start {
                path.push(((cur % side) as i32 + ox, (cur / side) as i32 + oy));
                cur = came[cur] as usize;
            }
            path.reverse();
            return Some(path.into_iter().map(|(x, y)| (x as u8, y as u8)).collect());
        }
        for (dx, dy) in game::DIRECTIONS {
            let (nx, ny) = (x + dx as i32, y + dy as i32);
            if !inside(nx, ny) || came[idx(nx, ny)] != u32::MAX || !monster_can_stand(t, nx as u8, ny as u8) {
                continue;
            }
            came[idx(nx, ny)] = idx(x, y) as u32;
            queue.push_back((nx, ny));
        }
    }
    None
}

/// xorshift64.
pub struct Rng(u64);

impl Default for Rng {
    fn default() -> Self {
        Self(0x9E37_79B9_7F4A_7C15)
    }
}

impl Rng {
    fn from_time() -> Self {
        Self(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_monster_txt() {
        let infos = parse_monster_txt(include_str!("../datos/Monster.txt"));
        let bull = &infos[&0];
        assert_eq!(bull.name, "Bull Fighter");
        assert_eq!((bull.level, bull.move_range, bull.view_range, bull.move_speed, bull.attack_speed), (6, 3, 5, 400, 1600));
        assert_eq!(infos[&3].name, "Spider");
    }

    #[test]
    fn lee_monster_set_base() {
        let rows = parse_setbase(include_str!("../datos/MonsterSetBase.txt"));
        // El entrenador de Lorencia, una zona de arañas y nada de otras secciones.
        assert!(rows.contains(&SpawnRow { section: 0, number: 226, map: 0, range: 0, x1: 122, y1: 110, x2: 122, y2: 110, dir: 3, count: 1 }));
        assert!(rows.iter().any(|r| r.section == 1 && r.number == 3 && r.map == 0 && (r.x1, r.y1, r.x2, r.y2) == (180, 90, 226, 244) && r.count == 45));
        assert!(rows.iter().all(|r| r.section <= 2));
        let lorencia: u32 = rows.iter().filter(|r| r.map == 0).map(|r| r.count as u32).sum();
        assert!(lorencia > 300, "{lorencia}");
    }

    #[test]
    fn secciones_de_eventos_se_saltan() {
        let rows = parse_setbase("0\n1 0 0 10 10 1\nend\n3\n5 0 0 1 1 9 9 -1 5\nend\n1\n2 0 30 20 30 10 40 -1 4 // al revés\nend\n");
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[1].x1, rows[1].y1, rows[1].x2, rows[1].y2), (10, 30, 20, 40));
    }

    /// Terreno todo pisable con una pared en x = 5 (de y 0 a 8) y zona segura en (0..3, 0..3).
    fn terreno_de_prueba() -> Grid {
        let mut t = Grid::default();
        for y in 0..9 {
            t.set_attribute(5, y, attr::NO_MOVE);
        }
        for y in 0..3 {
            for x in 0..3 {
                t.set_attribute(x, y, attr::SAFE_ZONE);
            }
        }
        t
    }

    #[test]
    fn camino_corto_rodea_paredes_y_no_pisa_zona_segura() {
        let t = terreno_de_prueba();
        let p = short_path(&t, (4, 4), (6, 4), (0, 0, 20, 20), 8).expect("camino");
        assert_eq!(p.last(), Some(&(6, 4)));
        assert!(p.iter().all(|&(x, y)| monster_can_stand(&t, x, y)));
        assert!(p.windows(2).all(|w| game::direction_between(w[0], w[1]).is_some()));
        // Sin salir de su zona no llega.
        assert_eq!(short_path(&t, (4, 4), (6, 4), (0, 0, 6, 6), 8), None);
        assert!(!monster_can_stand(&t, 1, 1));
    }

    #[test]
    fn pasean_solo_si_alguien_los_ve_y_sin_salir_de_su_zona() {
        let rows = [
            SpawnRow { section: 1, number: 3, map: 0, range: 30, x1: 10, y1: 10, x2: 14, y2: 14, dir: -1, count: 3 },
            SpawnRow { section: 0, number: 226, map: 0, range: 0, x1: 30, y1: 30, x2: 30, y2: 30, dir: 3, count: 1 },
        ];
        let infos = parse_monster_txt(include_str!("../datos/Monster.txt"));
        let start = Instant::now();
        let mut mobs = Mobs::build(&rows, &infos, HashMap::from([(0, terreno_de_prueba())]), start, Rng::default());
        assert_eq!(mobs.list.len(), 4);
        assert_eq!(mobs.get(4).map(|m| (m.number, m.x, m.y, m.dir)), Some((226, 30, 30, 3)));
        assert!(mobs.list[..3].iter().all(|m| (10..=14).contains(&m.x) && (10..=14).contains(&m.y)));

        // Nadie los ve: no se mueven.
        assert!(mobs.tick(start + Duration::from_secs(10), |_| false).is_empty());
        // Los ve alguien: pasean (el NPC no), por casillas seguidas y dentro de su zona.
        let mut walked = 0;
        for s in 0..200 {
            let now = start + Duration::from_secs(10) + Duration::from_millis(100 * s);
            for w in mobs.tick(now, |_| true) {
                assert_ne!(w.id, 4, "el NPC no anda");
                let ServerMessage::ObjectWalked { target_x, target_y, directions, .. } = w.message else { panic!() };
                assert!(!directions.is_empty() && directions.len() <= MAX_STEPS);
                assert!((10..=14).contains(&target_x) && (10..=14).contains(&target_y));
                walked += 1;
            }
            for m in &mobs.list[..3] {
                assert!((10..=14).contains(&m.x) && (10..=14).contains(&m.y), "se sale: ({}, {})", m.x, m.y);
            }
        }
        assert!(walked > 5, "{walked}");
    }

    #[test]
    fn el_paso_dura_move_speed() {
        let rows = [SpawnRow { section: 2, number: 0, map: 0, range: 3, x1: 10, y1: 10, x2: 10, y2: 10, dir: 0, count: 1 }];
        let infos = parse_monster_txt(include_str!("../datos/Monster.txt"));
        let start = Instant::now();
        let mut mobs = Mobs::build(&rows, &infos, HashMap::from([(0, Grid::default())]), start, Rng::default());
        let m = &mut mobs.list[0];
        m.path = VecDeque::from([(11, 10), (12, 10)]);
        m.next_step = start + Duration::from_millis(400);
        m.next_think = start + Duration::from_secs(100);
        mobs.tick(start + Duration::from_millis(399), |_| true);
        assert_eq!((mobs.list[0].x, mobs.list[0].y), (10, 10));
        mobs.tick(start + Duration::from_millis(400), |_| true);
        assert_eq!((mobs.list[0].x, mobs.list[0].y, mobs.list[0].dir), (11, 10, 3));
        assert_eq!(mobs.list[0].scope().target_x, 12);
        mobs.tick(start + Duration::from_millis(800), |_| true);
        assert_eq!((mobs.list[0].x, mobs.list[0].y), (12, 10));
    }
}
