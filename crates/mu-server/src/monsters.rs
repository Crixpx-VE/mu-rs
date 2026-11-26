//! World monsters and NPCs: where they spawn (`datos/MonsterSetBase.txt`, see `tools/gen_spawns.py`), their
//! data (`datos/Monster.txt`) and how they roam.
//!
//! - NPCs (section 0 of `MonsterSetBase`): on their tile, facing their direction, standing still.
//! - Zones (section 1): `Count` monsters on random tiles of the zone where they may spawn (walkable and
//!   outside the safe zone for monsters; see `free_tile`).
//! - Single spawns (section 2): on their tile.
//!
//! Roaming follows OpenMU's `BasicMonsterIntelligence.TickWithoutTargetAsync`: every `AttackSpeed`
//! milliseconds, if some player sees it and it is not walking, it goes to a random tile at most
//! `MoveRange` away on each axis (`Monster.RandomMoveAsync`), one step every `MoveSpeed` milliseconds (times the
//! step distance, `Monster.GetStepDelay`) and never onto the safe zone (`CanWalkOnSafezone`). At
//! most 15 steps per walk (as many as fit in the packet). It also never leaves its zone (single spawns: their
//! `Range` or, if 0, their `MoveRange` around their tile); OpenMU does not limit this, and over time the
//! monsters would spread over the whole map.
//!
//! Not yet: chasing and attacking (`ViewRange`, `AttackRange`), dying and respawning (`RegenTime`).

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::time::{Duration, Instant};

use mu_data::terrain::{self, attr};
use mu_protocol::game::{self, ScopeMonster, ServerMessage};

/// Monster object ids: from 1 up to this (players go above).
pub const MAX_MONSTERS: usize = 0x1FFF;

/// Max steps of one walk (`MAX_WALK_STEPS`).
const MAX_STEPS: usize = game::MAX_WALK_STEPS;

/// One row of `Monster.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub number: u16,
    pub name: String,
    /// OpenMU's `NpcObjectKind`: `Monster`, `PassiveNpc`, `Guard`, `Trap`, `Gate`, `Statue`...
    pub kind: String,
    pub level: u16,
    pub move_range: u8,
    pub view_range: u8,
    /// Milliseconds per tile.
    pub move_speed: u32,
    /// Milliseconds between one decision and the next (`AttackDelay` in OpenMU).
    pub attack_speed: u32,
}

impl Info {
    /// OpenMU's `NonPlayerCharacter.CanSpawnInSafezone`: everything but monsters and traps.
    pub fn can_spawn_in_safezone(&self) -> bool {
        self.kind != "Monster" && self.kind != "Trap"
    }
}

/// `Monster.txt` (written by `tools/gen_spawns.py` from OpenMU's `MonsterDefinition`s, see its header):
/// `Number "Name" Kind Level MaxHealth MinDamage MaxDamage Defense AttackRate DefenseRate MoveRange
/// AttackRange ViewRange MoveDelay AttackDelay RespawnDelay Attribute IceRes PoisonRes LightningRes FireRes Skill`,
/// delays in milliseconds.
pub fn parse_monster_txt(text: &str) -> HashMap<u16, Info> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        let (Some(a), Some(b)) = (line.find('"'), line.rfind('"')) else { continue };
        if a == b {
            continue;
        }
        let Ok(number) = line[..a].trim().parse::<u16>() else { continue };
        let rest: Vec<&str> = line[b + 1..].split_whitespace().collect();
        if rest.len() < 13 {
            continue;
        }
        let Ok(nums) = rest[1..13].iter().map(|w| w.parse::<f64>()).collect::<Result<Vec<_>, _>>() else { continue };
        out.insert(
            number,
            Info {
                number,
                name: line[a + 1..b].to_string(),
                kind: rest[0].to_string(),
                level: nums[0].clamp(0.0, u16::MAX as f64) as u16,
                move_range: nums[7].clamp(0.0, 255.0) as u8,
                view_range: nums[9].clamp(0.0, 255.0) as u8,
                move_speed: nums[10].max(0.0) as u32,
                attack_speed: nums[11].max(0.0) as u32,
            },
        );
    }
    out
}

/// One row of `MonsterSetBase.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnRow {
    /// 0 NPC, 1 zone, 2 single.
    pub section: u8,
    pub number: u16,
    pub map: u8,
    pub range: u8,
    pub x1: u8,
    pub y1: u8,
    pub x2: u8,
    pub y2: u8,
    /// 0..7, or -1 for random.
    pub dir: i8,
    pub count: u16,
}

/// Sections 0 (NPC), 1 (zones) and 2 (single); invasion and event sections are skipped.
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

/// A monster or NPC in the world.
pub struct Mob {
    pub id: u16,
    pub number: u16,
    pub map: u8,
    pub x: u8,
    pub y: u8,
    pub dir: u8,
    /// Zone it may roam in (inclusive): `(x1, y1, x2, y2)`.
    home: (u8, u8, u8, u8),
    /// NPCs and monsters with `MoveRange` 0 do not move.
    walks: bool,
    move_range: u8,
    move_speed: u32,
    attack_speed: u32,
    path: VecDeque<(u8, u8)>,
    next_step: Instant,
    next_think: Instant,
}

impl Mob {
    /// How a player who has it in view sees it right now.
    pub fn scope(&self) -> ScopeMonster {
        let (tx, ty) = self.path.back().copied().unwrap_or((self.x, self.y));
        ScopeMonster { id: self.id, number: self.number, x: self.x, y: self.y, target_x: tx, target_y: ty, rotation: self.dir }
    }

    /// How long the step to `to` takes (`MoveSpeed` times the distance, `Monster.GetStepDelay`).
    fn step_delay(&self, to: (u8, u8)) -> Duration {
        let diagonal = to.0 != self.x && to.1 != self.y;
        let ms = self.move_speed as f64 * if diagonal { std::f64::consts::SQRT_2 } else { 1.0 };
        Duration::from_micros((ms * 1000.0) as u64)
    }
}

/// A walk that starts: those who see the monster must see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Walked {
    pub id: u16,
    pub message: ServerMessage,
}

#[derive(Default)]
pub struct Mobs {
    /// By id: `list[id - 1]`.
    pub list: Vec<Mob>,
    terrains: HashMap<u8, Grid>,
    rng: Rng,
}

impl Mobs {
    /// Those from `datos/`, with the client terrain in `data` (`EncTerrainN.att`).
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
                    match free_tile(t, (row.x1, row.y1, row.x2, row.y2), info, &mut rng) {
                        Some(p) => p,
                        None => continue,
                    }
                } else {
                    (row.x1, row.y1)
                };
                // The zone it roams: its own; if it is a single tile, its `Range` (or its `MoveRange`) around it.
                let around = if row.range > 0 { row.range } else { info.move_range };
                let home = if (row.x1, row.y1) == (row.x2, row.y2) {
                    (row.x1.saturating_sub(around), row.y1.saturating_sub(around), row.x2.saturating_add(around), row.y2.saturating_add(around))
                } else {
                    (row.x1, row.y1, row.x2, row.y2)
                };
                let dir = if row.dir < 0 { (rng.next() % 8) as u8 } else { row.dir as u8 };
                // So they do not all decide at the same time.
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

    /// Advances to `now`: walking ones take their steps, and idle ones seen by someone (`observed(id)`)
    /// decide whether to roam. Returns the walks that start.
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

/// The tile flags of a map (`EncTerrainN.att`, see [`attr`]).
pub struct Grid(Vec<u16>);

impl Default for Grid {
    /// All walkable.
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

/// A monster can stand on a walkable, non-safe-zone tile.
fn monster_can_stand(t: &Grid, x: u8, y: u8) -> bool {
    t.walkable(x as usize, y as usize) && t.attribute(x as usize, y as usize) & attr::SAFE_ZONE == 0
}

/// A random tile of the zone where `info` may spawn (OpenMU `NonPlayerCharacter.IsValidSpawnPoint`): not on
/// the safe zone unless [`Info::can_spawn_in_safezone`], and walkable for monsters and guards (the ones that walk).
fn free_tile(t: &Grid, (x1, y1, x2, y2): (u8, u8, u8, u8), info: &Info, rng: &mut Rng) -> Option<(u8, u8)> {
    let (w, h) = (x2 as u64 - x1 as u64 + 1, y2 as u64 - y1 as u64 + 1);
    let needs_walkable = info.kind == "Monster" || info.kind == "Guard";
    for _ in 0..100 {
        let (x, y) = (x1 + (rng.next() % w) as u8, y1 + (rng.next() % h) as u8);
        let safe = t.attribute(x as usize, y as usize) & attr::SAFE_ZONE != 0;
        if (info.can_spawn_in_safezone() || !safe) && (!needs_walkable || t.walkable(x as usize, y as usize)) {
            return Some((x, y));
        }
    }
    None
}

/// Shortest path in steps (excluding the start tile) from `from` to `to` without leaving `home` nor
/// `reach` tiles around `from`, over tiles where a monster can stand.
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
        assert_eq!((bull.name.as_str(), bull.kind.as_str()), ("Bull Fighter", "Monster"));
        assert_eq!((bull.level, bull.move_range, bull.view_range, bull.move_speed, bull.attack_speed), (6, 3, 5, 400, 1600));
        assert_eq!(infos[&3].name, "Spider");
        // NPCs too (OpenMU's NpcInitialization), with no walking values.
        let zyro = &infos[&568];
        assert_eq!((zyro.name.as_str(), zyro.kind.as_str(), zyro.move_range), ("Wandering Merchant Zyro", "PassiveNpc", 0));
        assert!(zyro.can_spawn_in_safezone() && !bull.can_spawn_in_safezone());
        assert_eq!(infos[&100].kind, "Trap");
    }

    #[test]
    fn lee_monster_set_base() {
        let rows = parse_setbase(include_str!("../datos/MonsterSetBase.txt"));
        // The Lorencia trainer, a spider zone and nothing from other sections.
        assert!(rows.contains(&SpawnRow { section: 0, number: 226, map: 0, range: 0, x1: 122, y1: 110, x2: 122, y2: 110, dir: 3, count: 1 }));
        assert!(rows.iter().any(|r| r.section == 1 && r.number == 3 && r.map == 0 && (r.x1, r.y1, r.x2, r.y2) == (180, 90, 226, 244) && r.count == 45));
        // OpenMU-only NPCs: Wandering Merchant Zyro in Lorencia and Devias.
        assert!(rows.contains(&SpawnRow { section: 0, number: 568, map: 0, range: 0, x1: 131, y1: 139, x2: 131, y2: 139, dir: 2, count: 1 }));
        assert!(rows.iter().any(|r| r.section == 0 && r.number == 568 && r.map == 2 && (r.x1, r.y1) == (225, 52)));
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

    /// All-walkable terrain with a wall at x = 5 (y 0 to 8) and a safe zone at (0..3, 0..3).
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
        // It cannot get there without leaving its zone.
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

        // Nobody sees them: they do not move.
        assert!(mobs.tick(start + Duration::from_secs(10), |_| false).is_empty());
        // Someone sees them: they roam (not the NPC), over adjacent tiles and inside their zone.
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
