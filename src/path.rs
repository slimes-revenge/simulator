//! Grid pathing: 8 directions, shortest route to the nearest of several goal cells.
//!
//! Stands in for the game's `GridPath` (A* Pathfinding Project on a `GridGraph` with eight
//! neighbours). Costs are octile like the A* grid (straight 10, diagonal 14); among goals the
//! route with the fewest cells wins, then the cheaper one, then the goal with the smaller
//! (x, y). No puddle penalties: v1 has no puddles on the floor.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub type Cell = (i32, i32);

pub fn chebyshev(a: Cell, b: Cell) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// The eight neighbours, in a fixed order (x then y, from -1 to 1).
pub fn neighbours(cell: Cell) -> impl Iterator<Item = Cell> {
    (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dy| (dx, dy)))
        .filter(|&d| d != (0, 0))
        .map(move |(dx, dy)| (cell.0 + dx, cell.1 + dy))
}

/// The cells from `start` (excluded) to the best goal (included), or `None` when no goal is
/// reachable or `start` is already a goal. `blocked` is never asked about `start`.
pub fn find(
    width: i32,
    height: i32,
    start: Cell,
    goals: &[Cell],
    blocked: impl Fn(Cell) -> bool,
) -> Option<Vec<Cell>> {
    if goals.contains(&start) {
        return None;
    }
    let size = (width * height) as usize;
    let index = |c: Cell| (c.1 * width + c.0) as usize;
    let inside = |c: Cell| c.0 >= 0 && c.1 >= 0 && c.0 < width && c.1 < height;
    let mut cost = vec![i32::MAX; size];
    let mut steps = vec![i32::MAX; size];
    let mut parent: Vec<Option<Cell>> = vec![None; size];
    let mut done = vec![false; size];
    cost[index(start)] = 0;
    steps[index(start)] = 0;
    // Dijkstra; ties between equal (cost, steps) settle in row-major order (y, then x).
    let mut queue = BinaryHeap::from([Reverse((0, 0, start.1, start.0))]);
    while let Some(Reverse((c, s, y, x))) = queue.pop() {
        let cell = (x, y);
        if done[index(cell)] || (c, s) != (cost[index(cell)], steps[index(cell)]) {
            continue;
        }
        done[index(cell)] = true;
        for next in neighbours(cell) {
            if !inside(next) || blocked(next) {
                continue;
            }
            let diagonal = next.0 != cell.0 && next.1 != cell.1;
            let c = cost[index(cell)] + if diagonal { 14 } else { 10 };
            let s = steps[index(cell)] + 1;
            let i = index(next);
            if (c, s) < (cost[i], steps[i]) {
                cost[i] = c;
                steps[i] = s;
                parent[i] = Some(cell);
                queue.push(Reverse((c, s, next.1, next.0)));
            }
        }
    }
    let goal = goals
        .iter()
        .copied()
        .filter(|&g| inside(g) && cost[index(g)] != i32::MAX)
        .min_by_key(|&g| (steps[index(g)], cost[index(g)], g.0, g.1))?;
    let mut route = vec![goal];
    while let Some(p) = parent[index(*route.last().unwrap())] {
        if p == start {
            break;
        }
        route.push(p);
    }
    route.reverse();
    Some(route)
}
