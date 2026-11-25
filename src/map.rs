//! Map levels: a generated chain of rooms the slime walks through to the exit, enemies roaming
//! in their rooms and walls blocking sight. Answers how many enemies engage the slime at once
//! and what each fight nets. Spec agreed with the balance session; rules in
//! `docs/map-levels.md`, format in `docs/report-format.md`.

use crate::data::{Design, Level, Liquid, MapSpec};
use crate::fight::{Board, Fight, FightResult, MapState, Rect, in_rect};
use crate::path::{Cell, chebyshev};
use crate::rng::Rng;
use rayon::prelude::*;
use std::collections::BTreeMap;

/// A generated map: walls, rooms west to east, start and exit, enemies as (kind, cell, room).
#[derive(Clone, Debug)]
pub struct Map {
    pub board: Board,
    pub rooms: Vec<Rect>,
    pub start: Cell,
    pub exit: Cell,
    /// (kind, cell, home room); blockers have no room.
    pub enemies: Vec<(String, Cell, Option<usize>)>,
    /// Water drip cells, in corridors and doorways free of blockers.
    pub drips: Vec<Cell>,
}

fn between(rng: &mut Rng, (min, max): (i32, i32)) -> i32 {
    min + rng.pick((max - min + 1) as usize) as i32
}

/// Rooms share their top row (y = 1). Between neighbours: a doorway in each room's wall and
/// `corridor` cells, all on one random row both rooms have. Start: the middle of room 1's west
/// side; exit: the middle of the last room's east side.
pub fn generate(m: &MapSpec, seed: u64) -> Map {
    let mut rng = Rng::new(seed ^ 0x5EED_F4A7);
    let sizes: Vec<(i32, i32)> = (0..m.rooms)
        .map(|_| {
            (
                between(&mut rng, m.room_side),
                between(&mut rng, m.room_side),
            )
        })
        .collect();
    let mut rooms = Vec::new();
    let mut x = 1;
    for &(w, h) in &sizes {
        rooms.push(((x, 1), (x + w - 1, h)));
        x += w + m.corridor + 2;
    }
    let width = x - m.corridor - 1;
    let height = sizes.iter().map(|s| s.1).max().unwrap_or(1) + 2;
    let mut walls = vec![true; (width * height) as usize];
    let mut dig = |c: Cell| walls[(c.1 * width + c.0) as usize] = false;
    for &((x0, y0), (x1, y1)) in &rooms {
        for y in y0..=y1 {
            for x in x0..=x1 {
                dig((x, y));
            }
        }
    }
    for k in 1..rooms.len() {
        let shared = sizes[k - 1].1.min(sizes[k].1);
        let row = 1 + rng.pick(shared as usize) as i32;
        for x in rooms[k - 1].1.0 + 1..rooms[k].0.0 {
            dig((x, row));
        }
    }
    let middle = |r: Rect| r.0.1 + (r.1.1 - r.0.1) / 2;
    let start = (rooms[0].0.0, middle(rooms[0]));
    let last = rooms[rooms.len() - 1];
    let exit = (last.1.0, middle(last));

    let mut enemies: Vec<(String, Cell, Option<usize>)> = Vec::new();
    for (k, &((x0, y0), (x1, y1))) in rooms.iter().enumerate() {
        let (lo, hi) = m.enemies_per_room;
        let count = lo + rng.pick(hi - lo + 1);
        for _ in 0..count {
            let free: Vec<Cell> = (y0..=y1)
                .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
                .filter(|&c| chebyshev(c, start) > m.vision && c != exit)
                .filter(|&c| enemies.iter().all(|e| e.1 != c))
                .collect();
            if free.is_empty() {
                break;
            }
            let cell = free[rng.pick(free.len())];
            let kind = m.enemy_kinds[rng.pick(m.enemy_kinds.len())].clone();
            enemies.push((kind, cell, Some(k)));
        }
    }
    // Blockers stand in doorways and corridors: open cells outside every room.
    let board = Board {
        width,
        height,
        walls: walls.clone(),
    };
    let mut passages: Vec<Cell> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|&c| board.open(c) && !rooms.iter().any(|&r| in_rect(r, c)))
        .collect();
    for _ in 0..m.blockers_per_level {
        if passages.is_empty() {
            break;
        }
        let cell = passages.remove(rng.pick(passages.len()));
        let kind = m.blockers[rng.pick(m.blockers.len())].clone();
        enemies.push((kind, cell, None));
    }
    let mut drips = Vec::new();
    for _ in 0..m.drips.map_or(0, |d| d.cells) {
        if passages.is_empty() {
            break;
        }
        drips.push(passages.remove(rng.pick(passages.len())));
    }
    Map {
        board: Board {
            width,
            height,
            walls,
        },
        rooms,
        start,
        exit,
        enemies,
        drips,
    }
}

/// One map level over `matrix.seeds` seeds.
#[derive(Clone, Debug, PartialEq)]
pub struct MapLevelResult {
    pub id: String,
    pub policy: String,
    /// Percent of seeds that reached the exit.
    pub end: f64,
    /// Average rooms entered.
    pub rooms: f64,
    pub rounds: f64,
    /// Average episodes per run.
    pub fights: f64,
    /// Percent of all episodes whose most aggroed enemies adjacent at once was 0, 1, 2, 3+.
    pub engaged: [f64; 4],
    /// Episode net summed over all runs / episodes; `None` without episodes.
    pub net_per_fight: Option<f64>,
    /// Average over seeds of the smallest body (start and after each episode).
    pub min_body: f64,
    pub worst_body: usize,
    /// Where runs ended without the exit: `room<k>:<cause>` → seeds.
    pub stops: BTreeMap<String, u64>,
}

pub fn run(d: &Design, level: &Level) -> MapLevelResult {
    let spec = level.map.as_ref().expect("a map level");
    let ld = level_design(d, level);
    let body = start_body(level);
    // Seeds run in parallel and are collected in seed order.
    let runs: Vec<_> = (0..d.matrix.seeds)
        .into_par_iter()
        .map(|seed| play(&ld, level, spec, &body, seed))
        .collect();
    summarize(level, &runs, d.matrix.seeds)
}

/// The design with the slime's capacity set to the level's.
pub fn level_design(d: &Design, level: &Level) -> Design {
    let mut ld = d.clone();
    ld.slime.capacity = level.capacity;
    ld
}

/// The level's `start`, or full Water.
pub fn start_body(level: &Level) -> Vec<Liquid> {
    level
        .start
        .clone()
        .unwrap_or_else(|| vec![Liquid::Water; level.capacity])
}

/// Totals over the seeds that played; `end` is a percent of `of` seeds (a ladder run counts
/// the seeds that died on earlier levels too).
pub fn summarize(level: &Level, runs: &[(FightResult, MapState)], of: u64) -> MapLevelResult {
    let (mut ends, mut rooms, mut rounds, mut min_sum, mut worst) = (0, 0, 0u64, 0, usize::MAX);
    let mut engaged = [0u64; 4];
    let (mut episodes, mut net_sum) = (0u64, 0i64);
    let mut stops = BTreeMap::new();
    for (result, state) in runs {
        ends += u64::from(result.won);
        rooms += state.rooms_entered;
        rounds += u64::from(result.rounds);
        min_sum += state.min_body;
        worst = worst.min(state.min_body);
        for e in &state.episodes {
            engaged[e.max_adjacent.min(3)] += 1;
            net_sum += e.net;
        }
        episodes += state.episodes.len() as u64;
        if !result.won {
            let cause = result.cause.clone().unwrap_or_else(|| "unknown".into());
            let room = state.rooms_entered.max(1);
            *stops.entry(format!("room{room}:{cause}")).or_insert(0) += 1;
        }
    }
    let n = runs.len().max(1) as f64;
    let pct = |x: u64, of: u64| {
        if of == 0 {
            0.0
        } else {
            round2(100.0 * x as f64 / of as f64)
        }
    };
    MapLevelResult {
        id: level.id.clone(),
        policy: level.policy.clone(),
        end: pct(ends, of),
        rooms: round2(rooms as f64 / n),
        rounds: round2(rounds as f64 / n),
        fights: round2(episodes as f64 / n),
        engaged: engaged.map(|e| pct(e, episodes)),
        net_per_fight: (episodes > 0).then(|| round2(net_sum as f64 / episodes as f64)),
        min_body: round2(min_sum as f64 / n),
        worst_body: if runs.is_empty() { 0 } else { worst },
        stops,
    }
}

/// One seed of a map level: the generated map, the slime at its start, `strafe` to the exit.
pub fn play(
    d: &Design,
    level: &Level,
    spec: &MapSpec,
    body: &[Liquid],
    seed: u64,
) -> (FightResult, MapState) {
    fight(d, level, spec, body, seed).run_map()
}

/// The fight a seed plays, ready to run (`--trace` uses it too).
pub fn fight<'a>(
    d: &'a Design,
    level: &Level,
    spec: &MapSpec,
    body: &[Liquid],
    seed: u64,
) -> Fight<'a> {
    let map = generate(spec, seed);
    let enemies: Vec<(&str, Cell)> = map
        .enemies
        .iter()
        .map(|(k, c, _)| (k.as_str(), *c))
        .collect();
    let mut f = Fight::new(
        d,
        seed,
        level.action_points,
        body.to_vec(),
        map.start,
        &enemies,
    );
    f.board = map.board;
    f.vision = spec.vision;
    f.max_rounds = d.rules.max_rounds * spec.rooms as u32;
    let homes = map.enemies.iter().map(|e| e.2).collect();
    let first = (spec.rooms - spec.surface_rooms) / 2;
    let surface = map.rooms[first..first + spec.surface_rooms].to_vec();
    let mut state = MapState::new(map.rooms, homes, map.exit, body.len());
    state.river = spec.river;
    state.river_liquids = spec.river_liquids.clone();
    state.drips = map
        .drips
        .iter()
        .map(|&c| (c, spec.drips.map_or(0, |d| d.units)))
        .collect();
    state.surface = surface;
    state.sun_every = spec.sun_every;
    // `kite` walks and holds corridors like `lure`; next to enemies it kites (src/fight.rs).
    state.lure = level.policy == "lure" || level.policy == "kite";
    f.policy = crate::fight::Policy::from_name(&level.policy, d);
    f.map = Some(state);
    f
}

/// Two decimals; `+ 0.0` turns -0.0 into 0.0.
fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0 + 0.0
}

/// The map as text: `#` wall, `.` floor, `S` start, `X` exit, `~` drip, an enemy by its kind's first letter.
pub fn ascii(map: &Map) -> String {
    let mut out = String::new();
    for y in 0..map.board.height {
        for x in 0..map.board.width {
            let c = (x, y);
            let ch = if let Some(e) = map.enemies.iter().find(|e| e.1 == c) {
                e.0.chars().next().unwrap_or('?')
            } else if c == map.start {
                'S'
            } else if map.drips.contains(&c) {
                '~'
            } else if c == map.exit {
                'X'
            } else if map.board.wall(c) {
                '#'
            } else {
                '.'
            };
            out.push(ch);
        }
        out.push('\n');
    }
    out
}
