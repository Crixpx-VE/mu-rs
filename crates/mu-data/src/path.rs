//! Pathfinding (A*) over the 256 x 256 tile grid.
//!
//! Used by the client (to move the character on click) and, later,
//! by the server (to validate that the path sent by the client is possible).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::terrain::SIZE;

/// MU's 8 directions, in protocol order (0 = (-1,-1), clockwise).
pub const DIRECTIONS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)];

/// Finds a path from `from` to `to` (both included). `walkable(x, y)` says whether a tile can be stepped on.
/// `max_nodes` limits the work (the original searches short paths).
pub fn find_path(
    from: (usize, usize),
    to: (usize, usize),
    walkable: impl Fn(usize, usize) -> bool,
    max_nodes: usize,
) -> Option<Vec<(usize, usize)>> {
    if from == to {
        return Some(vec![from]);
    }
    if !walkable(to.0, to.1) {
        return None;
    }
    let idx = |(x, y): (usize, usize)| y * SIZE + x;
    // "Octile" distance ×10: straight costs 10, diagonal 14.
    let h = |(x, y): (usize, usize)| {
        let dx = x.abs_diff(to.0) as u32;
        let dy = y.abs_diff(to.1) as u32;
        10 * dx.max(dy) + 4 * dx.min(dy)
    };

    let mut came_from = vec![u32::MAX; SIZE * SIZE];
    let mut cost = vec![u32::MAX; SIZE * SIZE];
    let mut open = BinaryHeap::new();
    cost[idx(from)] = 0;
    open.push(Reverse((h(from), 0u32, from)));
    let mut expanded = 0;

    while let Some(Reverse((_, g, cur))) = open.pop() {
        if cur == to {
            let mut path = vec![cur];
            let mut c = idx(cur);
            while c != idx(from) {
                c = came_from[c] as usize;
                path.push((c % SIZE, c / SIZE));
            }
            path.reverse();
            return Some(path);
        }
        if g > cost[idx(cur)] {
            continue;
        }
        expanded += 1;
        if expanded > max_nodes {
            return None;
        }
        for (dx, dy) in DIRECTIONS {
            let nx = cur.0 as i32 + dx;
            let ny = cur.1 as i32 + dy;
            if nx < 0 || ny < 0 || nx >= SIZE as i32 || ny >= SIZE as i32 {
                continue;
            }
            let n = (nx as usize, ny as usize);
            if !walkable(n.0, n.1) {
                continue;
            }
            // Don't cut corners diagonally between two blocked tiles.
            if dx != 0 && dy != 0 && !walkable(n.0, cur.1) && !walkable(cur.0, n.1) {
                continue;
            }
            let ng = g + if dx != 0 && dy != 0 { 14 } else { 10 };
            if ng < cost[idx(n)] {
                cost[idx(n)] = ng;
                came_from[idx(n)] = idx(cur) as u32;
                open.push(Reverse((ng + h(n), ng, n)));
            }
        }
    }
    None
}

/// `MAX_PATH_FIND` (_define.h:75): `PATH::GetPath` keeps at most this many tiles, the start included.
pub const MAX_PATH_FIND: usize = 15;

/// `PathFinding2(sx, sy, tx, ty, &Path)` with its default arguments (`fDistance` 0, `iDefaultWall`
/// `TW_CHARACTER`; ZzzAI.cpp:727-775), a port of the original `PATH` search (ZzzPath.h:162-412).
///
/// `wall(x, y)` is `TerrainWall` (0..256 each), including the `TW_CHARACTER` mark that
/// `MoveCharactersClient` puts on the tile of every live character (ZzzCharacter.cpp:6317-6332).
///
/// - First a strict search (500 tests) that only succeeds if the target tile itself is reachable.
/// - If it fails, a loose one (50 tests) that returns the path to the tested tile closest to the
///   target (`EstimateCostToGoal`). Before it, in or toward a safe zone, characters stop blocking
///   (`TW_NOMOVE` as the wall) unless the target tile holds one. This is how a click on an NPC (whose
///   tile is `TW_CHARACTER`) walks to a tile next to it.
///
/// Returns the first [`MAX_PATH_FIND`] tiles (start included) if there are at least two (`PathNum > 1`),
/// `None` otherwise (the caller then acts in place, e.g. `Action(c, o, true)`).
pub fn path_finding2(
    start: (usize, usize),
    target: (usize, usize),
    wall: impl Fn(usize, usize) -> u16,
) -> Option<Vec<(usize, usize)>> {
    use crate::terrain::attr;
    let found = find_path_original(start, target, true, attr::CHARACTER, &wall).or_else(|| {
        // `TERRAIN_INDEX_REPEAT`.
        let (s, t) = (wall(start.0 & 255, start.1 & 255), wall(target.0 & 255, target.1 & 255));
        let safe = s & attr::SAFE_ZONE != 0 || t & attr::SAFE_ZONE != 0;
        let w = if safe && t & attr::CHARACTER == 0 { attr::NO_MOVE } else { attr::CHARACTER };
        find_path_original(start, target, false, w, &wall)
    })?;
    let n = found.len().min(MAX_PATH_FIND);
    (n > 1).then(|| found[..n].to_vec())
}

/// `PATH::FindPath` with `fDistance` 0 and `Value` false (ZzzPath.h:162-299).
///
/// The open list is the original `CBTree` (BaseCls.h:645-691): a binary tree where equal costs go
/// right and the leftmost node is tested next, i.e. lowest cost first and, among equal costs, the
/// one added first. That is reproduced with a heap ordered by (cost, insertion order).
fn find_path_original(
    start: (usize, usize),
    end: (usize, usize),
    error_check: bool,
    wall: u16,
    map: &impl Fn(usize, usize) -> u16,
) -> Option<Vec<(usize, usize)>> {
    use crate::terrain::attr;
    // `PATH_INTESTLIST`, `PATH_TESTED`, `PATH_END`, `MAX_INT_FORPATH`, `FACTOR_PATH_DIST(_DIAG)`
    // (_define.h:95-103).
    const IN_TEST_LIST: u8 = 0x01;
    const TESTED: u8 = 0x02;
    const END: u8 = 0x04;
    const MAX_INT: i64 = 65000 * 30000;
    const DIST: i64 = 5;
    const DIST_DIAG: i64 = (5.0f32 * 1.414) as i64;
    // `PATH::s_iDir` (zzzpath.cpp:10-15).
    const DIRS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
    // `EstimateCostToGoal` (ZzzPath.h:379-389).
    let estimate = |(xe, ye): (i32, i32), (x, y): (i32, i32)| -> i64 {
        let xd = (x - xe).abs() as i64;
        let mut yd = (y - ye).abs() as i64;
        if xd == 1 && yd == 1 {
            yd = 0;
        }
        ((xd - yd).abs() * DIST + xd.min(yd) * DIST_DIAG + 1) * 3 / 4
    };
    let inside = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < SIZE && (y as usize) < SIZE;
    let index = |x: i32, y: i32| y as usize * SIZE + x as usize;

    if start.0 == 0 || start.1 == 0 {
        return None;
    }
    let (s, e) = ((start.0 as i32, start.1 as i32), (end.0 as i32, end.1 as i32));
    if !inside(e.0, e.1) || !inside(s.0, s.1) {
        return None;
    }
    let mut closed = vec![0u8; SIZE * SIZE];
    let mut cost = vec![MAX_INT; SIZE * SIZE];
    let mut prev = vec![usize::MAX; SIZE * SIZE];
    let end_wall = map(end.0, end.1);
    if error_check && wall <= end_wall && end_wall & attr::ACTION == 0 {
        return None;
    }
    closed[index(e.0, e.1)] = END;

    let mut open = BinaryHeap::new();
    let mut order = 0u32;
    open.push(Reverse((0i64, order, index(s.0, s.1))));
    closed[index(s.0, s.1)] |= IN_TEST_LIST;

    let generate = |to: (i32, i32), prev: &[usize]| -> Option<Vec<(usize, usize)>> {
        // `GeneratePath` (ZzzPath.h:391-412): back from `to` along `m_pxPrev`, at most `MAX_COUNT_PATH`.
        let mut path = Vec::new();
        let mut cur = index(to.0, to.1);
        for _ in 0..500 {
            path.push((cur % SIZE, cur / SIZE));
            if cur == index(s.0, s.1) {
                path.reverse();
                return Some(path);
            }
            cur = *prev.get(cur)?;
        }
        None
    };

    let max_count = if error_check { 500 } else { 50 };
    let (mut nearest_cost, mut nearest) = (MAX_INT, s);
    let mut check = max_count;
    while check > 0 {
        let Some(Reverse((_, _, i))) = open.pop() else { break };
        let t = ((i % SIZE) as i32, (i / SIZE) as i32);
        cost[i] = if check == max_count { 0 } else { MAX_INT };
        for (dx, dy) in DIRS {
            let (x, y) = (t.0 + dx, t.1 + dy);
            if !inside(x, y) {
                continue;
            }
            let n = index(x, y);
            if closed[n] & TESTED != 0 {
                let c = cost[n] + if dx == 0 || dy == 0 { DIST } else { DIST_DIAG };
                if c < cost[i] {
                    cost[i] = c;
                    prev[i] = n;
                }
            }
        }
        closed[i] |= TESTED;
        if closed[i] & END != 0 {
            return generate(t, &prev);
        }
        for (dx, dy) in DIRS {
            let (x, y) = (t.0 + dx, t.1 + dy);
            if !inside(x, y) {
                continue;
            }
            let n = index(x, y);
            let a = map(x as usize, y as usize) & !(attr::ACTION | attr::HEIGHT | attr::CAMERA_UP);
            if closed[n] & IN_TEST_LIST == 0 && wall > a {
                order += 1;
                open.push(Reverse((cost[i] + estimate(e, (x, y)), order, n)));
                closed[n] |= IN_TEST_LIST;
                prev[n] = i;
            }
        }
        if !error_check {
            let c = estimate(e, t);
            if c < nearest_cost {
                nearest_cost = c;
                nearest = t;
            }
        }
        check -= 1;
    }
    if !error_check {
        return generate(nearest, &prev);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::attr;

    /// Open ground with a character (an NPC) on `npc`.
    fn ground_with(npc: (usize, usize)) -> impl Fn(usize, usize) -> u16 {
        move |x, y| if (x, y) == npc { attr::CHARACTER } else { 0 }
    }

    #[test]
    fn hacia_un_npc_se_para_al_lado() {
        let npc = (13, 10);
        let p = path_finding2((10, 10), npc, ground_with(npc)).unwrap();
        assert_eq!(p.first(), Some(&(10, 10)));
        let last = *p.last().unwrap();
        assert_eq!(last.0.abs_diff(npc.0).max(last.1.abs_diff(npc.1)), 1, "ends next to the NPC: {p:?}");
        assert!(!p.contains(&npc));
        for w in p.windows(2) {
            assert!(w[0].0.abs_diff(w[1].0) <= 1 && w[0].1.abs_diff(w[1].1) <= 1);
        }
    }

    #[test]
    fn junto_al_npc_no_hay_camino() {
        // Already next to it (straight or diagonal): `PathNum` 1, so `Action(c, o, true)` right away.
        for hero in [(12, 10), (12, 9), (14, 11)] {
            assert_eq!(path_finding2(hero, (13, 10), ground_with((13, 10))), None, "{hero:?}");
        }
    }

    #[test]
    fn destino_libre_y_corte_a_15() {
        let free = |_: usize, _: usize| 0;
        let p = path_finding2((10, 10), (14, 10), free).unwrap();
        assert_eq!((p.first(), p.last()), (Some(&(10, 10)), Some(&(14, 10))), "{p:?}");
        for w in p.windows(2) {
            assert!(w[0].0.abs_diff(w[1].0) <= 1 && w[0].1.abs_diff(w[1].1) <= 1);
        }
        // Far away the strict search runs out of tests and the loose one heads toward it, cut to 15.
        let long = path_finding2((10, 10), (60, 10), free).unwrap();
        assert!(long.len() <= MAX_PATH_FIND && long.last().unwrap().0 > 10, "{long:?}");
        assert_eq!(path_finding2((0, 10), (14, 10), free), None, "`xStart == 0`");
    }

    #[test]
    fn rodea_un_muro() {
        // Vertical wall at x=5 from y=0 to y=9, with a gap at y=10.
        let walkable = |x: usize, y: usize| !(x == 5 && y < 10);
        let p = find_path((2, 2), (8, 2), walkable, 10_000).unwrap();
        assert_eq!(p.first(), Some(&(2, 2)));
        assert_eq!(p.last(), Some(&(8, 2)));
        assert!(p.iter().all(|&(x, y)| walkable(x, y)));
        assert!(p.iter().any(|&(_, y)| y >= 10), "tiene que pasar por el hueco");
        for w in p.windows(2) {
            assert!(w[0].0.abs_diff(w[1].0) <= 1 && w[0].1.abs_diff(w[1].1) <= 1, "pasos de una casilla");
        }
    }

    #[test]
    fn destino_bloqueado() {
        assert!(find_path((0, 0), (3, 3), |x, y| (x, y) != (3, 3), 1000).is_none());
    }
}
