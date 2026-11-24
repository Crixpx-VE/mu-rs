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

#[cfg(test)]
mod tests {
    use super::*;

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
