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

/// Every cheapest route from one start: one Dijkstra answers routes to any number of goals.
pub struct Tree {
    width: i32,
    height: i32,
    start: Cell,
    cost: Vec<i32>,
    steps: Vec<i32>,
    parent: Vec<Option<Cell>>,
}

impl Tree {
    /// Dijkstra from `start`; `blocked` is never asked about `start`. Ties between equal
    /// (cost, steps) settle in row-major order (y, then x).
    pub fn new(width: i32, height: i32, start: Cell, blocked: impl Fn(Cell) -> bool) -> Tree {
        let size = (width * height) as usize;
        let mut t = Tree {
            width,
            height,
            start,
            cost: vec![i32::MAX; size],
            steps: vec![i32::MAX; size],
            parent: vec![None; size],
        };
        if !t.inside(start) {
            return t;
        }
        let mut done = vec![false; size];
        let s = t.index(start);
        t.cost[s] = 0;
        t.steps[s] = 0;
        let mut queue = BinaryHeap::from([Reverse((0, 0, start.1, start.0))]);
        while let Some(Reverse((c, n, y, x))) = queue.pop() {
            let cell = (x, y);
            let i = t.index(cell);
            if done[i] || (c, n) != (t.cost[i], t.steps[i]) {
                continue;
            }
            done[i] = true;
            for next in neighbours(cell) {
                if !t.inside(next) || blocked(next) {
                    continue;
                }
                let diagonal = next.0 != cell.0 && next.1 != cell.1;
                let nc = c + if diagonal { 14 } else { 10 };
                let ns = n + 1;
                let j = t.index(next);
                if (nc, ns) < (t.cost[j], t.steps[j]) {
                    t.cost[j] = nc;
                    t.steps[j] = ns;
                    t.parent[j] = Some(cell);
                    queue.push(Reverse((nc, ns, next.1, next.0)));
                }
            }
        }
        t
    }

    fn index(&self, c: Cell) -> usize {
        (c.1 * self.width + c.0) as usize
    }

    fn inside(&self, c: Cell) -> bool {
        c.0 >= 0 && c.1 >= 0 && c.0 < self.width && c.1 < self.height
    }

    /// Cells on the route to `c`, or `None` when it cannot be reached.
    pub fn steps_to(&self, c: Cell) -> Option<i32> {
        (self.inside(c) && self.cost[self.index(c)] != i32::MAX).then(|| self.steps[self.index(c)])
    }

    /// The cells from the start (excluded) to the best goal (included), or `None` when no goal
    /// is reachable or the start is already a goal.
    pub fn route(&self, goals: &[Cell]) -> Option<Vec<Cell>> {
        if goals.contains(&self.start) {
            return None;
        }
        let goal = goals
            .iter()
            .copied()
            .filter(|&g| self.steps_to(g).is_some())
            .min_by_key(|&g| {
                let i = self.index(g);
                (self.steps[i], self.cost[i], g.0, g.1)
            })?;
        let mut route = vec![goal];
        while let Some(p) = self.parent[self.index(*route.last().unwrap())] {
            if p == self.start {
                break;
            }
            route.push(p);
        }
        route.reverse();
        Some(route)
    }
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
    Tree::new(width, height, start, blocked).route(goals)
}
