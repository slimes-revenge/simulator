//! One fight by the game's rules: board, creatures, statuses, strikes, enemy AI, turn order.
//!
//! Each function names the C# method it mirrors (slimes-revenge/SlimesRevenge,
//! `Assets/Scripts/Runtime`). Rules notes: `docs/rules-model.md`.

use crate::data::{Design, Liquid, Personality, Trait};
use crate::path::{self, Cell, chebyshev};
use crate::rng::Rng;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusKind {
    Wet,
    Burning,
    Poisoned,
    Corroding {
        corrosion: i32,
    },
    Instability,
    Flammable,
    /// Blood dominance: +Blood each pulse; cleared when dominance changes.
    Regeneration,
}

impl StatusKind {
    pub fn name(self) -> &'static str {
        match self {
            StatusKind::Wet => "Wet",
            StatusKind::Burning => "Burning",
            StatusKind::Poisoned => "Poisoned",
            StatusKind::Corroding { .. } => "Corroding",
            StatusKind::Instability => "Instability",
            StatusKind::Flammable => "Flammable",
            StatusKind::Regeneration => "Regeneration",
        }
    }
}

/// Where a status came from, for the damage slice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Strike,
    Retaliation,
    /// The slime's own dominance (Regeneration).
    Body,
}

#[derive(Clone, Copy, Debug)]
pub struct Status {
    pub kind: StatusKind,
    pub source: Source,
    /// Pulses left; `FOREVER` never expires (`StatusEffect.Forever`).
    pub remaining: i32,
    id: u32,
}

pub const FOREVER: i32 = -1;

#[derive(Clone, Debug)]
pub struct Creature {
    pub name: String,
    pub is_slime: bool,
    pub cell: Cell,
    pub hit_points: i32,
    pub max_hit_points: i32,
    pub armor: i32,
    pub base_speed: i32,
    pub melee_power: i32,
    /// The slime's body and life; for enemies, the corpse's units.
    pub volume: Vec<Liquid>,
    /// `StatusQueue`, FIFO. Same kinds stack: two Burnings both pulse.
    pub statuses: Vec<Status>,
    pub personality: Personality,
    pub vampirism: bool,
    /// Slime only: the liquid with at least `dominance_percent` of the volume.
    pub dominant: Option<Liquid>,
    pub aggroed: bool,
    pub pursuit: Option<Cell>,
    pub pursuit_left: i32,
    pub flee_cell: Option<Cell>,
}

impl Creature {
    pub fn alive(&self) -> bool {
        if self.is_slime {
            !self.volume.is_empty()
        } else {
            self.hit_points > 0
        }
    }

    /// `Creature.Speed`: base (at least 1) plus status modifiers, never below 0.
    fn speed(&self, d: &Design) -> i32 {
        let modifier: i32 = self
            .statuses
            .iter()
            .filter(|s| s.kind == StatusKind::Instability)
            .map(|_| d.statuses.instability_speed)
            .sum();
        (self.base_speed.max(1) + modifier).max(0)
    }

    fn count(&self, liquid: Liquid) -> usize {
        self.volume.iter().filter(|&&l| l == liquid).count()
    }
}

/// The grid a fight is played on: open cells unless `walls` marks them.
#[derive(Clone, Debug)]
pub struct Board {
    pub width: i32,
    pub height: i32,
    /// Row-major, `width × height`; empty means no walls.
    pub walls: Vec<bool>,
}

impl Board {
    pub fn inside(&self, c: Cell) -> bool {
        c.0 >= 0 && c.1 >= 0 && c.0 < self.width && c.1 < self.height
    }

    pub fn wall(&self, c: Cell) -> bool {
        !self.walls.is_empty() && self.inside(c) && self.walls[(c.1 * self.width + c.0) as usize]
    }

    /// Floor: inside and not a wall.
    pub fn open(&self, c: Cell) -> bool {
        self.inside(c) && !self.wall(c)
    }

    /// No wall on the Bresenham line strictly between `a` and `b`.
    pub fn line_of_sight(&self, a: Cell, b: Cell) -> bool {
        if self.walls.is_empty() {
            return true;
        }
        let (dx, dy) = ((b.0 - a.0).abs(), -(b.1 - a.1).abs());
        let (sx, sy) = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
        let (mut x, mut y, mut err) = (a.0, a.1, dx + dy);
        loop {
            if (x, y) == b {
                return true;
            }
            if (x, y) != a && self.wall((x, y)) {
                return false;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }
}

/// An inclusive rectangle of cells: (min corner, max corner).
pub type Rect = (Cell, Cell);

fn in_rect(r: Rect, c: Cell) -> bool {
    c.0 >= r.0.0 && c.0 <= r.1.0 && c.1 >= r.0.1 && c.1 <= r.1.1
}

/// A map run: rooms, the exit to reach, and the fights ("episodes") on the way.
#[derive(Clone, Debug, Default)]
pub struct MapState {
    pub rooms: Vec<Rect>,
    /// Each enemy's room; it wanders only there until aggroed.
    pub homes: Vec<Option<usize>>,
    pub exit: Cell,
    /// `lure` instead of `strafe`.
    pub lure: bool,
    /// Actions the lure policy has waited on its way into the next room.
    waited: u32,
    pub rooms_entered: usize,
    pub episodes: Vec<Episode>,
    /// The smallest body at the start and after each closed episode.
    pub min_body: usize,
    open: Option<OpenEpisode>,
    /// Counters and living enemies as of the last check with no episode open: the action that
    /// opens an episode (a first strike, a first bite) belongs to it.
    calm: Option<OpenEpisode>,
}

/// One fight on a map: from the first enemy aggroed on the slime until none is.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Episode {
    /// The most aggroed enemies adjacent to the slime at once.
    pub max_adjacent: usize,
    /// corpse + grown − spent − bitten − status during the episode, ignoring capacity.
    pub net: i64,
    pub corpse: usize,
    pub eaten: usize,
    pub wasted: usize,
}

impl MapState {
    /// The slime starts in room 1 with a body of `body` units.
    pub fn new(rooms: Vec<Rect>, homes: Vec<Option<usize>>, exit: Cell, body: usize) -> MapState {
        MapState {
            rooms,
            homes,
            exit,
            rooms_entered: 1,
            episodes: Vec::new(),
            min_body: body,
            open: None,
            calm: None,
            lure: false,
            waited: 0,
        }
    }
}

#[derive(Clone, Debug)]
struct OpenEpisode {
    max_adjacent: usize,
    /// spent, bitten, status, grown when the episode opened.
    counters: (usize, usize, usize, usize),
    alive_at_open: Vec<bool>,
}

/// How the slime chooses its strikes (matrix and stage levels; map levels walk by their own
/// policy and strike like `Newcomer`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Policy {
    /// Strikes with whatever is on the edge; never swaps.
    Newcomer,
    /// Before a strike, at most one swap(0, 1) by the balance session's rules (`arranger_swap`).
    Arranger { keep: Option<Liquid> },
}

impl Policy {
    pub fn from_name(name: &str, d: &Design) -> Policy {
        match name {
            "arranger" => Policy::Arranger {
                keep: d.policies.keep(),
            },
            _ => Policy::Newcomer,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Intent {
    Attack,
    Chase,
    Flee,
    Wander,
    Idle,
}

/// Per-fight breakdown for the [economy] and [damage] slices.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Tally {
    /// Units spent on strikes, by liquid.
    pub spent_by: BTreeMap<Liquid, usize>,
    /// Units popped by bites, by liquid.
    pub bitten_by: BTreeMap<Liquid, usize>,
    /// Slime units lost to its own status pulses, by status.
    pub status_lost_by: BTreeMap<&'static str, usize>,
    /// Enemy HP removed by strikes.
    pub strike_dmg: usize,
    /// Enemy HP removed by pulses of statuses the slime's strikes applied.
    pub status_dmg: usize,
    /// Enemy HP removed by pulses of statuses Retaliation applied.
    pub retaliation_dmg: usize,
    /// Enemy HP removed by status pulses (any source), by status.
    pub status_dmg_by: BTreeMap<&'static str, usize>,
    /// Damage beyond the HP the target had at that moment, any source.
    pub overkill: usize,
    /// Strikes on an enemy its queued status pulses alone would already kill.
    pub doomed_strikes: usize,
    /// Actions spent swapping two units of the body.
    pub swaps: usize,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct FightResult {
    pub won: bool,
    pub rounds: u32,
    pub units_left: usize,
    pub units_spent: usize,
    /// Units the slime lost to enemy bites.
    pub units_bitten: usize,
    /// Units the slime lost to its own status pulses.
    pub units_status: usize,
    /// Blood units the slime grew by Regeneration.
    pub units_grown: usize,
    pub enemies_killed: usize,
    /// Why the slime lost: `melee:<Enemy>`, `status:<Status>` or `timeout`. `None` on a win.
    pub cause: Option<String>,
    pub tally: Tally,
}

pub struct Fight<'a> {
    d: &'a Design,
    pub board: Board,
    /// Enemy sight range (Chebyshev); `rules.vision_range` unless a map level sets it.
    pub vision: i32,
    pub max_rounds: u32,
    /// Set for map levels: the slime walks to the exit (`strafe`, `lure`) instead of hunting.
    pub map: Option<MapState>,
    pub policy: Policy,
    /// `arranger` swapped before its pending strike: the next strike action strikes.
    swapped: bool,
    rng: Rng,
    actions_per_round: u32,
    pub slime: Creature,
    pub enemies: Vec<Creature>,
    next_status_id: u32,
    units_spent: usize,
    units_bitten: usize,
    units_status: usize,
    units_grown: usize,
    tally: Tally,
    /// What last took a unit off the slime.
    last_harm: Option<String>,
    /// Turn-by-turn log lines, kept only for `--trace`.
    trace: Option<Vec<String>>,
}

impl<'a> Fight<'a> {
    /// The slime with `volume` at `slime_cell`; enemies as (kind, cell).
    pub fn new(
        d: &'a Design,
        seed: u64,
        actions_per_round: u32,
        volume: Vec<Liquid>,
        slime_cell: Cell,
        enemies: &[(&str, Cell)],
    ) -> Fight<'a> {
        let slime = Creature {
            name: "Slime".into(),
            is_slime: true,
            cell: slime_cell,
            hit_points: 0,
            max_hit_points: 0,
            armor: 0,
            base_speed: d.slime.speed,
            melee_power: 0,
            volume: volume.into_iter().take(d.slime.capacity).collect(),
            statuses: Vec::new(),
            personality: Personality::Aggressive,
            vampirism: false,
            dominant: None,
            aggroed: false,
            pursuit: None,
            pursuit_left: 0,
            flee_cell: None,
        };
        let enemies = enemies
            .iter()
            .map(|&(kind, cell)| {
                let e = d.enemy(kind);
                Creature {
                    name: e.name.clone(),
                    is_slime: false,
                    cell,
                    hit_points: e.hit_points,
                    max_hit_points: e.hit_points,
                    armor: e.armor,
                    base_speed: e.speed,
                    melee_power: e.melee_power,
                    volume: e.volume.clone(),
                    statuses: Vec::new(),
                    personality: e.personality,
                    vampirism: e.traits.contains(&Trait::Vampirism),
                    dominant: None,
                    aggroed: false,
                    pursuit: None,
                    pursuit_left: 0,
                    flee_cell: None,
                }
            })
            .collect();
        let mut fight = Fight {
            d,
            board: Board {
                width: d.rules.board_width,
                height: d.rules.board_height,
                walls: Vec::new(),
            },
            vision: d.rules.vision_range,
            max_rounds: d.rules.max_rounds,
            map: None,
            policy: Policy::Newcomer,
            swapped: false,
            rng: Rng::new(seed),
            actions_per_round,
            slime,
            enemies,
            next_status_id: 0,
            units_spent: 0,
            units_bitten: 0,
            units_status: 0,
            units_grown: 0,
            tally: Tally::default(),
            last_harm: None,
            trace: None,
        };
        let d = fight.d;
        refresh_dominance(d, &mut fight.slime, &mut fight.next_status_id);
        fight
    }

    /// Plays the fight to the end. Round: the slime acts `actions_per_round` times, then each
    /// enemy pulses its statuses and takes one intent, then the slime's statuses pulse
    /// (`GameSession.Resolve`). `TurnManager.Bind` pulses the slime once before its first action.
    pub fn run(self) -> FightResult {
        self.play().0
    }

    /// Plays the fight and returns the slime's body at the end as well (level runs).
    pub fn run_with_volume(mut self) -> (FightResult, Vec<Liquid>) {
        let result = self.rounds();
        (result, self.slime.volume)
    }

    /// Plays the fight and returns a turn-by-turn log as well (`--trace`).
    pub fn run_traced(mut self) -> (FightResult, Vec<String>) {
        self.trace = Some(Vec::new());
        self.play()
    }

    fn play(mut self) -> (FightResult, Vec<String>) {
        self.note(|f| format!("start: {}", f.describe_all()));
        let result = self.rounds();
        self.note(|_| format!("result: {result:?}"));
        (result, self.trace.unwrap_or_default())
    }

    fn note(&mut self, line: impl FnOnce(&Fight) -> String) {
        if let Some(mut trace) = self.trace.take() {
            trace.push(line(self));
            self.trace = Some(trace);
        }
    }

    fn label(&self, i: usize) -> String {
        format!("{}#{}", self.enemies[i].name, i + 1)
    }

    fn describe_all(&self) -> String {
        let s = &self.slime;
        let mut out = format!(
            "Slime {:?} volume [{}] dominant {:?} statuses {:?}",
            s.cell,
            s.volume
                .iter()
                .map(|l| format!("{l:?}"))
                .collect::<Vec<_>>()
                .join(" "),
            s.dominant,
            s.statuses
                .iter()
                .map(|x| (x.kind, x.remaining))
                .collect::<Vec<_>>()
        );
        for (i, e) in self.enemies.iter().enumerate().filter(|(_, e)| e.alive()) {
            out += &format!(
                "; {} {:?} HP {}/{} armor {} aggro {} statuses {:?}",
                self.label(i),
                e.cell,
                e.hit_points,
                e.max_hit_points,
                e.armor,
                e.aggroed,
                e.statuses
                    .iter()
                    .map(|x| (x.kind, x.remaining))
                    .collect::<Vec<_>>()
            );
        }
        out
    }

    fn rounds(&mut self) -> FightResult {
        self.pulse(None);
        self.track_engagement();
        if !self.slime.alive() {
            return self.result(false, 0);
        }
        for round in 1..=self.max_rounds {
            self.note(|_| format!("round {round}"));
            for _ in 0..self.actions_per_round {
                self.slime_act();
                self.track_engagement();
                if self.goal_reached() {
                    return self.result(true, round);
                }
            }
            for i in 0..self.enemies.len() {
                if !self.enemies[i].alive() {
                    continue;
                }
                self.pulse(Some(i));
                if !self.enemies[i].alive() {
                    continue;
                }
                self.enemy_turn(i);
                self.track_engagement();
                if !self.slime.alive() {
                    return self.result(false, round);
                }
            }
            if self.goal_reached() {
                return self.result(true, round);
            }
            self.pulse(None);
            self.track_engagement();
            if !self.slime.alive() {
                return self.result(false, round);
            }
            self.note(|f| format!("end of round {round}: {}", f.describe_all()));
        }
        self.last_harm = Some("timeout".into());
        self.result(false, self.max_rounds)
    }

    /// Matrix fights end when every enemy is dead; map runs when the slime stands on the exit.
    fn goal_reached(&self) -> bool {
        match &self.map {
            Some(m) => self.slime.alive() && self.slime.cell == m.exit,
            None => self.enemies_dead(),
        }
    }

    /// Map runs only: opens an episode when an enemy is aggroed on the slime, tracks how many
    /// aggroed enemies stand next to it at once, closes it when none is left.
    fn track_engagement(&mut self) {
        let Some(m) = self.map.as_ref() else { return };
        let engaged = |e: &Creature| e.alive() && e.aggroed;
        let aggro = self.enemies.iter().any(engaged);
        let adjacent = self
            .enemies
            .iter()
            .filter(|e| engaged(e) && chebyshev(e.cell, self.slime.cell) == 1)
            .count();
        let counters = (
            self.units_spent,
            self.units_bitten,
            self.units_status,
            self.units_grown,
        );
        let alive: Vec<bool> = self.enemies.iter().map(|e| e.alive()).collect();
        let open = m.open.is_some();
        let m = self.map.as_mut().expect("checked");
        let now = OpenEpisode {
            max_adjacent: adjacent,
            counters,
            alive_at_open: alive,
        };
        if let Some(o) = m.open.as_mut() {
            o.max_adjacent = o.max_adjacent.max(adjacent);
        } else if aggro {
            let calm = m.calm.take().unwrap_or_else(|| now.clone());
            m.open = Some(OpenEpisode {
                max_adjacent: adjacent,
                ..calm
            });
        } else {
            m.calm = Some(now.clone());
        }
        if open && !aggro {
            self.close_episode();
            if let Some(m) = self.map.as_mut() {
                m.calm = Some(now);
            }
        }
    }

    /// Ends the open episode; a living slime eats the corpses of enemies that died in it.
    fn close_episode(&mut self) {
        let Some(o) = self.map.as_mut().and_then(|m| m.open.take()) else {
            return;
        };
        let (spent, bitten, status, grown) = o.counters;
        let mut ep = Episode {
            max_adjacent: o.max_adjacent,
            ..Episode::default()
        };
        if self.slime.alive() {
            for (e, &was_alive) in self.enemies.iter().zip(&o.alive_at_open) {
                if was_alive && !e.alive() {
                    for &unit in &e.volume {
                        ep.corpse += 1;
                        if self.slime.volume.len() < self.d.slime.capacity {
                            self.slime.volume.push(unit);
                            ep.eaten += 1;
                        } else {
                            ep.wasted += 1;
                        }
                    }
                }
            }
            let d = self.d;
            refresh_dominance(d, &mut self.slime, &mut self.next_status_id);
        }
        ep.net = (ep.corpse + self.units_grown - grown) as i64
            - (self.units_spent - spent + self.units_bitten - bitten + self.units_status - status)
                as i64;
        let body = self.slime.volume.len();
        let m = self.map.as_mut().expect("an episode was open");
        m.min_body = m.min_body.min(body);
        m.episodes.push(ep);
    }

    /// Plays a map run and returns its map state (episodes, rooms entered) as well.
    pub fn run_map(mut self) -> (FightResult, MapState) {
        self.track_engagement();
        let result = self.rounds();
        self.close_episode();
        let mut state = self.map.take().unwrap_or_default();
        if !self.slime.alive() {
            state.min_body = 0;
        }
        (result, state)
    }

    fn result(&self, won: bool, rounds: u32) -> FightResult {
        FightResult {
            won,
            rounds,
            units_left: self.slime.volume.len(),
            units_spent: self.units_spent,
            units_bitten: self.units_bitten,
            units_status: self.units_status,
            units_grown: self.units_grown,
            tally: self.tally.clone(),
            enemies_killed: self.enemies.iter().filter(|e| !e.alive()).count(),
            cause: if won { None } else { self.last_harm.clone() },
        }
    }

    fn enemies_dead(&self) -> bool {
        self.enemies.iter().all(|e| !e.alive())
    }

    fn occupied(&self) -> Vec<Cell> {
        std::iter::once(&self.slime)
            .chain(&self.enemies)
            .filter(|c| c.alive())
            .map(|c| c.cell)
            .collect()
    }

    fn inside(&self, cell: Cell) -> bool {
        self.board.open(cell)
    }

    /// What a walker cannot step into: walls and other creatures.
    fn blocked(&self, occupied: &[Cell], c: Cell) -> bool {
        occupied.contains(&c) || self.board.wall(c)
    }

    /// `CreatureMoves.ApproachCells` → `GridPath.TryWalk`: the cell `speed` steps along the
    /// route to a free cell next to `target`, or `None` when there is no route.
    fn walk_toward(&self, from: Cell, target: Cell, speed: i32) -> Option<Cell> {
        if speed < 1 {
            return None;
        }
        let occupied = self.occupied();
        let free = |c: Cell| self.inside(c) && !occupied.contains(&c);
        let mut goals: Vec<Cell> = path::neighbours(target)
            .filter(|&c| c == from || free(c))
            .collect();
        if goals.is_empty() && free(target) {
            goals.push(target);
        }
        let route = path::find(self.board.width, self.board.height, from, &goals, |c| {
            self.blocked(&occupied, c)
        })?;
        Some(route[(speed as usize).min(route.len()) - 1])
    }

    // ---- the slime: newcomer policy --------------------------------------------------------

    /// Newcomer: walk to the nearest enemy, strike it with the edge unit, wait when the last
    /// unit cannot be spent. Never swaps. Uses only player actions (step, attack, wait).
    fn slime_act(&mut self) {
        if self.map.is_some() {
            return self.map_act();
        }
        let Some(target) = self
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, e)| e.alive())
            .min_by_key(|(i, e)| (chebyshev(self.slime.cell, e.cell), *i))
            .map(|(i, _)| i)
        else {
            return;
        };
        if chebyshev(self.slime.cell, self.enemies[target].cell) == 1 {
            // `Volume.CanSpend`: the last unit is the slime's life.
            if self.slime.volume.len() > 1 && !self.swapped && self.arranger_swap(target) {
                self.slime_swap(0);
                self.swapped = true;
                self.note(|f| format!("Slime swaps: {:?} to the edge", f.edge()));
            } else if self.slime.volume.len() > 1 {
                self.swapped = false;
                let liquid = self.edge();
                let hp = self.enemies[target].hit_points;
                self.slime_strike(target, liquid);
                self.note(|f| {
                    let e = &f.enemies[target];
                    format!(
                        "Slime strikes {} with {liquid:?}: HP {hp}->{}, armor {}, statuses {:?}",
                        f.label(target),
                        e.hit_points,
                        e.armor,
                        e.statuses
                            .iter()
                            .map(|x| (x.kind, x.remaining))
                            .collect::<Vec<_>>()
                    )
                });
            } else {
                self.note(|_| "Slime waits: the last unit cannot be spent".into());
            }
            return;
        }
        let from = self.slime.cell;
        if let Some(to) = self.walk_toward(from, self.enemies[target].cell, 1) {
            self.slime.cell = to;
            self.note(|_| format!("Slime steps {from:?}->{to:?}"));
        } else {
            self.note(|_| "Slime waits: no path".into());
        }
    }

    /// Map runs. Both policies strike an aggroed enemy next to the slime first.
    /// - `strafe`: otherwise step toward the exit; when the way is blocked, strike whatever
    ///   stands next to it. Never hunts.
    /// - `lure`: while any enemy is aggroed, hold the nearest corridor cell and let them come
    ///   one at a time; when calm, advance, but wait outside the next room (up to
    ///   `policies.lure_patience` actions per approach) while 2+ calm Aggressive enemies would
    ///   see the next cell.
    fn map_act(&mut self) {
        let adjacent = |f: &Fight, aggroed_only: bool| {
            f.enemies.iter().position(|e| {
                e.alive() && chebyshev(e.cell, f.slime.cell) == 1 && (e.aggroed || !aggroed_only)
            })
        };
        if let Some(i) = adjacent(self, true) {
            return self.strike_or_wait(i);
        }
        let m = self.map.as_ref().expect("map runs");
        let (lure, exit, waited) = (m.lure, m.exit, m.waited);
        let goal = if lure && self.enemies.iter().any(|e| e.alive() && e.aggroed) {
            self.map.as_mut().expect("checked").waited = 0;
            match self.fighting_spot() {
                Some(spot) if spot != self.slime.cell => spot,
                _ => return self.note(|_| "Slime holds the corridor".into()),
            }
        } else {
            if lure && !self.in_room(self.slime.cell) && waited < self.d.policies.lure_patience {
                let next = self.route_to(exit).map(|r| r[0]);
                if next.is_some_and(|c| self.calm_watchers(c) >= 2) {
                    self.map.as_mut().expect("checked").waited += 1;
                    return self.note(|_| "Slime waits at the doorway: 2+ would see it".into());
                }
            }
            exit
        };
        if let Some(route) = self.route_to(goal) {
            let (from, to) = (self.slime.cell, route[0]);
            self.slime.cell = to;
            let m = self.map.as_mut().expect("checked");
            if let Some(k) = m.rooms.iter().position(|&r| in_rect(r, to)) {
                m.rooms_entered = m.rooms_entered.max(k + 1);
                m.waited = 0;
            }
            self.note(|_| format!("Slime steps {from:?}->{to:?}"));
        } else if let Some(i) = adjacent(self, false) {
            self.strike_or_wait(i);
        } else {
            self.note(|_| "Slime waits: no path to the exit".into());
        }
    }

    fn in_room(&self, c: Cell) -> bool {
        self.map
            .as_ref()
            .is_some_and(|m| m.rooms.iter().any(|&r| in_rect(r, c)))
    }

    /// The slime's route to `goal` around walls and creatures.
    fn route_to(&self, goal: Cell) -> Option<Vec<Cell>> {
        let occupied = self.occupied();
        path::find(
            self.board.width,
            self.board.height,
            self.slime.cell,
            &[goal],
            |c| self.blocked(&occupied, c),
        )
    }

    /// Calm Aggressive enemies that would see the slime standing on `cell` (and so engage it).
    fn calm_watchers(&self, cell: Cell) -> usize {
        let hidden = self.slime.dominant == Some(Liquid::Mercury);
        self.enemies
            .iter()
            .filter(|e| e.alive() && !e.aggroed && e.personality == Personality::Aggressive)
            .filter(|e| !hidden && chebyshev(e.cell, cell) <= self.vision)
            .filter(|e| self.board.line_of_sight(e.cell, cell))
            .count()
    }

    /// Where `lure` fights: the nearest non-room cell, preferring corridor cells with at most
    /// two open neighbours (one enemy can reach it from each side) over doorways; ties go west.
    fn fighting_spot(&self) -> Option<Cell> {
        let mut best: Option<((bool, usize, i32), Cell)> = None;
        for y in 0..self.board.height {
            for x in 0..self.board.width {
                let c = (x, y);
                if !self.board.open(c) || self.in_room(c) {
                    continue;
                }
                let steps = if c == self.slime.cell {
                    0
                } else {
                    match self.route_to(c) {
                        Some(r) => r.len(),
                        None => continue,
                    }
                };
                let open = path::neighbours(c).filter(|&n| self.board.open(n)).count();
                let key = (open > 2, steps, x);
                if best.is_none_or(|(k, _)| key < k) {
                    best = Some((key, c));
                }
            }
        }
        best.map(|(_, c)| c)
    }

    fn strike_or_wait(&mut self, target: usize) {
        if self.slime.volume.len() > 1 {
            let liquid = self.edge();
            self.slime_strike(target, liquid);
            self.note(|f| format!("Slime strikes {} with {liquid:?}", f.label(target)));
        } else {
            self.note(|_| "Slime waits: the last unit cannot be spent".into());
        }
    }

    /// `arranger`: whether to swap(0, 1) before striking `target`. The first rule that applies:
    /// 1. keep the kept liquid deep: the edge is it and unit 1 is not;
    /// 2. no water on fire: the edge is Water, the target burns or corrodes, unit 1 is not Water;
    /// 3. Lava on Flammable: unit 1 is Lava, the edge is not, the target is Flammable.
    fn arranger_swap(&self, target: usize) -> bool {
        let Policy::Arranger { keep } = self.policy else {
            return false;
        };
        let n = self.slime.volume.len();
        if n < 2 {
            return false;
        }
        let (edge, next) = (self.slime.volume[n - 1], self.slime.volume[n - 2]);
        let has =
            |k: fn(&StatusKind) -> bool| self.enemies[target].statuses.iter().any(|s| k(&s.kind));
        (keep == Some(edge) && next != edge)
            || (edge == Liquid::Water
                && next != Liquid::Water
                && has(|k| matches!(k, StatusKind::Burning | StatusKind::Corroding { .. })))
            || (next == Liquid::Lava
                && edge != Liquid::Lava
                && has(|k| *k == StatusKind::Flammable))
    }

    /// The unit a strike spends: the edge of the body (the end bites pop too).
    fn edge(&self) -> Liquid {
        *self
            .slime
            .volume
            .last()
            .expect("the slime holds at least one unit")
    }

    /// `swap`: exchanges the units at positions `i` and `i + 1`, counted from the edge
    /// (0 = the edge unit), and counts it. A policy that swaps spends one action on it (as on a
    /// step); no policy swaps yet, so this hook spends none by itself.
    pub fn slime_swap(&mut self, i: usize) {
        let n = self.slime.volume.len();
        if i < n.saturating_sub(1) {
            self.slime.volume.swap(n - 1 - i, n - 2 - i);
            self.tally.swaps += 1;
        }
    }

    /// `TurnManager.TryAttack` → `Combat.Attack(attacker, target, substance)`.
    pub fn slime_strike(&mut self, target: usize, liquid: Liquid) {
        let d = self.d;
        // The unit of that kind nearest the edge: the edge unit itself in every policy.
        let Some(at) = self.slime.volume.iter().rposition(|&l| l == liquid) else {
            return;
        };
        self.slime.volume.remove(at);
        self.units_spent += 1;
        *self.tally.spent_by.entry(liquid).or_insert(0) += 1;
        refresh_dominance(d, &mut self.slime, &mut self.next_status_id);

        let stats = d.liquid(liquid);
        let enemy = &mut self.enemies[target];
        if pending_status_damage(d, enemy) >= enemy.hit_points {
            self.tally.doomed_strikes += 1;
        }
        enemy.armor = (enemy.armor - stats.corrosion).max(0);
        let power = if liquid == Liquid::Lava {
            modify_incoming_harm(d, enemy, stats.power)
        } else {
            stats.power
        };
        let hp = enemy.hit_points;
        damage(d, enemy, power, true, &mut self.next_status_id);
        let applied = (hp - enemy.hit_points) as usize;
        self.tally.strike_dmg += applied;
        self.tally.overkill += (power - enemy.armor.min(power)) as usize - applied;
        if enemy.alive() {
            apply_liquid(d, enemy, liquid, Source::Strike, &mut self.next_status_id);
        }
        enemy.aggroed = true;
    }

    // ---- enemies ---------------------------------------------------------------------------

    /// `Creature.TakeTurn`: decide by personality, redirect, perform, after-act.
    fn enemy_turn(&mut self, i: usize) {
        let mut intent = self.decide(i);
        // `CreatureHunt.Redirect`: no fear/hate traits in the live roster, so only pursuit memory.
        if matches!(intent, Intent::Wander | Intent::Idle) {
            let e = &mut self.enemies[i];
            e.flee_cell = None;
            if e.pursuit.is_some() && e.pursuit_left > 0 {
                intent = Intent::Chase;
            } else {
                e.pursuit = None;
                e.pursuit_left = 0;
            }
        }
        let (from, volume) = (self.enemies[i].cell, self.slime.volume.len());
        match intent {
            Intent::Attack => self.enemy_melee(i),
            Intent::Chase => self.chase(i),
            Intent::Flee => self.flee(i),
            Intent::Wander => self.wander(i),
            Intent::Idle => {}
        }
        self.note(|f| {
            let e = &f.enemies[i];
            format!(
                "{} {intent:?}: {from:?}->{:?}, slime volume {volume}->{}, HP {}",
                f.label(i),
                e.cell,
                f.slime.volume.len(),
                e.hit_points
            )
        });
        self.after_act(i, intent);
    }

    /// `IsDetectedBy`: an aggroed observer always detects; Mercury dominance hides the slime.
    fn detected(&self, i: usize) -> bool {
        self.slime.alive()
            && (self.enemies[i].aggroed || self.slime.dominant != Some(Liquid::Mercury))
    }

    fn can_see(&self, i: usize) -> bool {
        chebyshev(self.enemies[i].cell, self.slime.cell) <= self.vision
            && self.detected(i)
            && self
                .board
                .line_of_sight(self.enemies[i].cell, self.slime.cell)
    }

    fn adjacent_and_detected(&self, i: usize) -> bool {
        chebyshev(self.enemies[i].cell, self.slime.cell) == 1 && self.detected(i)
    }

    /// `CreaturePolicyGraphs`: the Aggressive / Passive / Cowardly trees.
    fn decide(&mut self, i: usize) -> Intent {
        let e = &self.enemies[i];
        match e.personality {
            Personality::Aggressive => self.aggressive_tree(i),
            Personality::Passive if e.aggroed => self.aggressive_tree(i),
            Personality::Passive => self.wander_or_idle(),
            Personality::Cowardly => {
                if e.aggroed && self.adjacent_and_detected(i) {
                    return Intent::Attack;
                }
                self.enemies[i].aggroed = false;
                if self.can_see(i) {
                    Intent::Flee
                } else {
                    self.wander_or_idle()
                }
            }
        }
    }

    fn aggressive_tree(&mut self, i: usize) -> Intent {
        if self.adjacent_and_detected(i) {
            Intent::Attack
        } else if self.enemies[i].aggroed || self.can_see(i) {
            Intent::Chase
        } else {
            self.wander_or_idle()
        }
    }

    fn wander_or_idle(&mut self) -> Intent {
        if self.rng.wander_rather_than_idle() {
            Intent::Wander
        } else {
            Intent::Idle
        }
    }

    /// `Combat.Attack(attacker, target)`: a bare melee hit on the slime.
    fn enemy_melee(&mut self, i: usize) {
        if !self.detected(i) {
            return;
        }
        let d = self.d;
        // Retaliation is captured before the hit: the hit may end dominance.
        let retort = self.slime.dominant;
        let power = self.enemies[i].melee_power;
        let before = self.slime.volume.clone();
        let tip = damage(d, &mut self.slime, power, true, &mut self.next_status_id);
        self.units_bitten += before.len() - self.slime.volume.len();
        for &unit in &before[self.slime.volume.len()..] {
            *self.tally.bitten_by.entry(unit).or_insert(0) += 1;
        }
        if tip.is_some() {
            self.last_harm = Some(format!("melee:{}", self.enemies[i].name));
        }
        let enemy = &mut self.enemies[i];
        if self.slime.alive() {
            if let Some(liquid) = retort {
                apply_liquid(
                    d,
                    enemy,
                    liquid,
                    Source::Retaliation,
                    &mut self.next_status_id,
                );
            }
            enemy.aggroed = true;
            enemy.pursuit = Some(self.slime.cell);
            enemy.pursuit_left = d.rules.pursuit_memory_turns;
        }
        // `Vampirism.OnOwnerStruckVolumeTip`: runs even when the hit killed the slime.
        if tip == Some(Liquid::Blood) && enemy.vampirism {
            heal(enemy, d.statuses.vampirism_heal);
        }
    }

    /// `CreatureMoves.TryChase`.
    fn chase(&mut self, i: usize) {
        let target = if self.can_see(i) {
            let e = &mut self.enemies[i];
            e.pursuit = Some(self.slime.cell);
            e.pursuit_left = self.d.rules.pursuit_memory_turns;
            self.slime.cell
        } else {
            self.enemies[i].pursuit.unwrap_or(self.slime.cell)
        };
        let e = &self.enemies[i];
        if let Some(to) = self.walk_toward(e.cell, target, e.speed(self.d)) {
            self.enemies[i].cell = to;
        }
    }

    /// `CreatureMoves.TryWander`: one random free neighbour.
    fn wander(&mut self, i: usize) {
        if self.enemies[i].speed(self.d) < 1 {
            return;
        }
        let occupied = self.occupied();
        let home = self
            .map
            .as_ref()
            .and_then(|m| m.homes.get(i).copied().flatten().map(|k| m.rooms[k]));
        let here = self.enemies[i].cell;
        let mut options: Vec<Cell> = path::neighbours(here)
            .filter(|&c| self.inside(c) && !occupied.contains(&c))
            // Inside its room it stays there; outside it (fled, lost aggro) it walks freely.
            .filter(|&c| home.is_none_or(|r| !in_rect(r, here) || in_rect(r, c)))
            .collect();
        if options.is_empty() {
            return;
        }
        options.sort();
        self.enemies[i].cell = options[self.rng.pick(options.len())];
    }

    /// `CreatureMoves.TryFlee`: walk toward the free cell farthest from the slime; if that
    /// route does not gain distance, take any neighbour that does.
    fn flee(&mut self, i: usize) {
        let threat = self.slime.cell;
        let occupied = self.occupied();
        let from = self.enemies[i].cell;
        let speed = self.enemies[i].speed(self.d);
        let flee_cell_ok = self.enemies[i]
            .flee_cell
            .is_some_and(|c| !occupied.contains(&c) && c != from);
        if !flee_cell_ok {
            // `CreatureHunt.PickFleeCell`: far from the threat, then near to self.
            let mut best = (from, -1);
            for y in 0..self.board.height {
                for x in 0..self.board.width {
                    let c = (x, y);
                    if self.blocked(&occupied, c) {
                        continue;
                    }
                    let score = chebyshev(c, threat) * 1000 - chebyshev(c, from);
                    if score > best.1 {
                        best = (c, score);
                    }
                }
            }
            self.enemies[i].flee_cell = Some(best.0);
        }
        let goal = self.enemies[i].flee_cell.unwrap_or(from);
        if goal == from || speed < 1 {
            return;
        }
        let distance = chebyshev(from, threat);
        let route = path::find(self.board.width, self.board.height, from, &[goal], |c| {
            self.blocked(&occupied, c)
        });
        if let Some(route) = route {
            let to = route[(speed as usize).min(route.len()) - 1];
            if chebyshev(to, threat) > distance {
                self.enemies[i].cell = to;
                return;
            }
        }
        let away: Vec<Cell> = (-1..=1)
            .flat_map(|y| (-1..=1).map(move |x| (from.0 + x, from.1 + y)))
            .filter(|&c| c != from && self.inside(c) && !occupied.contains(&c))
            .filter(|&c| chebyshev(c, threat) > distance)
            .collect();
        if !away.is_empty() {
            self.enemies[i].cell = away[self.rng.pick(away.len())];
        }
    }

    /// `CreatureHunt.AfterAct` for the slime as focus: sticky aggro and pursuit memory.
    fn after_act(&mut self, i: usize, intent: Intent) {
        let sees = self.can_see(i);
        let adjacent = chebyshev(self.enemies[i].cell, self.slime.cell) == 1;
        let slime_cell = self.slime.cell;
        let memory = self.d.rules.pursuit_memory_turns;
        let e = &mut self.enemies[i];
        match intent {
            Intent::Flee => {
                if e.flee_cell == Some(e.cell) {
                    e.flee_cell = None;
                }
                if adjacent {
                    e.flee_cell = None;
                    e.aggroed = true;
                }
            }
            Intent::Chase | Intent::Attack => {
                if intent == Intent::Attack || sees {
                    e.aggroed = true;
                }
                if sees {
                    e.pursuit = Some(slime_cell);
                    e.pursuit_left = memory;
                } else if let Some(p) = e.pursuit {
                    if chebyshev(e.cell, p) <= 1 {
                        e.pursuit = None;
                        e.pursuit_left = 0;
                    } else {
                        e.pursuit_left -= 1;
                        if e.pursuit_left <= 0 {
                            e.pursuit = None;
                            e.pursuit_left = 0;
                        }
                    }
                }
            }
            Intent::Wander | Intent::Idle => {}
        }
    }

    // ---- statuses --------------------------------------------------------------------------

    /// `StatusQueue.Pulse` for the slime (`None`) or an enemy: each status once, FIFO, at the
    /// start of its owner's turn; stops when the owner dies.
    fn pulse(&mut self, enemy: Option<usize>) {
        let d = self.d;
        let who = enemy.map_or("Slime".to_string(), |i| self.label(i));
        let mut log: Option<Vec<String>> = self.trace.as_ref().map(|_| Vec::new());
        let ids = &mut self.next_status_id;
        let owner = match enemy {
            Some(i) => &mut self.enemies[i],
            None => &mut self.slime,
        };
        let mut pulsed: Vec<u32> = Vec::new();
        while pulsed.len() < owner.statuses.len() + 8 {
            let Some(at) = owner.statuses.iter().position(|s| !pulsed.contains(&s.id)) else {
                break;
            };
            let status = owner.statuses[at];
            pulsed.push(status.id);
            let (before, hp) = (owner.volume.len(), owner.hit_points);
            // HP (or units) this pulse means to take, before it meets what the owner has left.
            let mut harm = 0;
            match status.kind {
                StatusKind::Burning => {
                    harm = modify_incoming_harm(d, owner, d.statuses.burning_pulse);
                    damage(d, owner, harm, false, ids);
                }
                StatusKind::Poisoned => {
                    harm = d.statuses.poisoned_pulse;
                    damage(d, owner, harm, false, ids);
                }
                StatusKind::Corroding { corrosion } => {
                    if owner.armor > 0 {
                        owner.armor = (owner.armor - corrosion).max(0);
                    } else {
                        harm = d.statuses.corroding_pulse;
                        damage(d, owner, harm, false, ids);
                    }
                }
                StatusKind::Regeneration => {
                    for _ in 0..d.statuses.regeneration_growth {
                        if owner.volume.len() < d.slime.capacity {
                            owner.volume.push(Liquid::Blood);
                        }
                    }
                }
                StatusKind::Wet | StatusKind::Instability | StatusKind::Flammable => {}
            }
            if owner.is_slime && owner.volume.len() > before {
                self.units_grown += owner.volume.len() - before;
            }
            if owner.is_slime && owner.volume.len() < before {
                let lost = before - owner.volume.len();
                self.units_status += lost;
                *self
                    .tally
                    .status_lost_by
                    .entry(status.kind.name())
                    .or_insert(0) += lost;
                self.last_harm = Some(format!("status:{}", status.kind.name()));
            }
            if !owner.is_slime && harm > 0 {
                let applied = (hp - owner.hit_points) as usize;
                let t = &mut self.tally;
                match status.source {
                    Source::Retaliation => t.retaliation_dmg += applied,
                    Source::Strike | Source::Body => t.status_dmg += applied,
                }
                *t.status_dmg_by.entry(status.kind.name()).or_insert(0) += applied;
                t.overkill += harm as usize - applied;
            }
            if let Some(log) = log.as_mut() {
                log.push(format!(
                    "{who} pulse {:?}: HP {hp}->{}, volume {before}->{}, armor {}",
                    status.kind,
                    owner.hit_points,
                    owner.volume.len(),
                    owner.armor
                ));
            }
            // A pulse may have removed the status (dominance change); tick it where it is now.
            if let Some(now) = owner.statuses.iter().position(|s| s.id == status.id) {
                let s = &mut owner.statuses[now];
                if s.remaining > 0 {
                    s.remaining -= 1;
                }
                if s.remaining == 0 {
                    owner.statuses.remove(now);
                }
            }
            if !owner.alive() {
                break;
            }
            refresh_dominance(d, owner, ids);
        }
        if let (Some(trace), Some(log)) = (self.trace.as_mut(), log) {
            trace.extend(log);
        }
    }
}

/// `Creature.Damage`: the slime loses units from the end of its volume (returns the first one
/// lost, the "tip"); others lose hit points, reduced by armor when `blocked_by_armor`.
fn damage(
    d: &Design,
    c: &mut Creature,
    amount: i32,
    blocked_by_armor: bool,
    ids: &mut u32,
) -> Option<Liquid> {
    if amount <= 0 || !c.alive() {
        return None;
    }
    if c.is_slime {
        let mut tip = None;
        for _ in 0..amount {
            let Some(unit) = c.volume.pop() else { break };
            tip.get_or_insert(unit);
        }
        if c.alive() {
            refresh_dominance(d, c, ids);
        }
        return tip;
    }
    let dealt = if blocked_by_armor {
        amount - c.armor.min(amount)
    } else {
        amount
    };
    c.hit_points = (c.hit_points - dealt).max(0);
    None
}

fn heal(c: &mut Creature, amount: i32) {
    if c.alive() {
        c.hit_points = (c.hit_points + amount).min(c.max_hit_points);
    }
}

/// HP the queued statuses of `c` would take if they all pulsed out as they stand now: Flammable
/// as it is now, Corroding stripping armor first, no heals.
fn pending_status_damage(d: &Design, c: &Creature) -> i32 {
    let mut armor = c.armor;
    let mut total = 0;
    for s in &c.statuses {
        // Forever statuses never pulse damage in the model (only Regeneration, on the slime).
        let pulses = s.remaining.max(0);
        match s.kind {
            StatusKind::Burning => {
                total += pulses * modify_incoming_harm(d, c, d.statuses.burning_pulse);
            }
            StatusKind::Poisoned => total += pulses * d.statuses.poisoned_pulse,
            StatusKind::Corroding { corrosion } => {
                for _ in 0..pulses {
                    if armor > 0 {
                        armor = (armor - corrosion).max(0);
                    } else {
                        total += d.statuses.corroding_pulse;
                    }
                }
            }
            _ => {}
        }
    }
    total
}

/// `Creature.ModifyIncomingHarm`: ×multiplier per Flammable in the queue, then the Oil
/// dominance's Forever Flammable unless a Flammable is already queued.
fn modify_incoming_harm(d: &Design, c: &Creature, mut amount: i32) -> i32 {
    if amount <= 0 {
        return amount;
    }
    let mut queued = false;
    for s in &c.statuses {
        if s.kind == StatusKind::Flammable {
            amount *= d.statuses.flammable_multiplier;
            queued = true;
        }
    }
    if c.dominant == Some(Liquid::Oil) && !queued {
        amount *= d.statuses.flammable_multiplier;
    }
    amount
}

/// `Substance.Apply`: what a liquid leaves on whoever it touches (strike, retaliation, puddle),
/// then `NotifyReceivedSubstance` (Vampirism drinks Blood).
fn apply_liquid(d: &Design, target: &mut Creature, liquid: Liquid, from: Source, ids: &mut u32) {
    let s = &d.statuses;
    let mut add = |kind, duration| add_status(target, kind, duration, from, ids);
    match liquid {
        Liquid::Water => add(StatusKind::Wet, s.wet),
        Liquid::Oil => {
            add(StatusKind::Instability, s.instability);
            add(StatusKind::Flammable, s.flammable);
        }
        Liquid::Poison => add(StatusKind::Poisoned, s.poisoned),
        Liquid::Acid => {
            let corrosion = d.liquid(Liquid::Acid).corrosion;
            add(StatusKind::Corroding { corrosion }, s.corroding);
        }
        Liquid::Lava => add(StatusKind::Burning, s.burning),
        Liquid::Blood | Liquid::Mercury => {}
    }
    if liquid == Liquid::Blood && target.vampirism {
        heal(target, s.vampirism_heal);
    }
}

/// `StatusQueue.Add`: Water dominance blocks Burning; Wet and Burning/Corroding cancel each
/// other (the existing one is removed and the incoming one is not added).
fn add_status(c: &mut Creature, kind: StatusKind, duration: i32, source: Source, ids: &mut u32) {
    if kind == StatusKind::Burning && c.dominant == Some(Liquid::Water) {
        return;
    }
    let cancels = |existing: StatusKind| {
        matches!(
            (existing, kind),
            (StatusKind::Burning, StatusKind::Wet)
                | (StatusKind::Corroding { .. }, StatusKind::Wet)
                | (StatusKind::Wet, StatusKind::Burning)
        )
    };
    let before = c.statuses.len();
    c.statuses.retain(|s| !cancels(s.kind));
    if c.statuses.len() < before {
        return;
    }
    *ids += 1;
    c.statuses.push(Status {
        kind,
        source,
        remaining: duration,
        id: *ids,
    });
}

/// `VolumeDominance.Refresh` (slime only): on a change of dominant liquid, statuses owned by
/// the old one go (Regeneration); Blood queues Regeneration first; Water puts out Burning.
fn refresh_dominance(d: &Design, c: &mut Creature, ids: &mut u32) {
    if !c.is_slime {
        return;
    }
    let mut best: Option<(Liquid, usize)> = None;
    for &unit in &c.volume {
        let n = c.count(unit);
        if best.is_none_or(|(_, b)| n > b) {
            best = Some((unit, n));
        }
    }
    let next = best
        .filter(|&(_, n)| n * 100 >= c.volume.len() * d.rules.dominance_percent)
        .map(|(l, _)| l);
    if next == c.dominant {
        return;
    }
    c.statuses.retain(|s| s.kind != StatusKind::Regeneration);
    c.dominant = next;
    match next {
        Some(Liquid::Blood) => {
            *ids += 1;
            c.statuses.insert(
                0,
                Status {
                    kind: StatusKind::Regeneration,
                    source: Source::Body,
                    remaining: FOREVER,
                    id: *ids,
                },
            );
        }
        Some(Liquid::Water) => c.statuses.retain(|s| s.kind != StatusKind::Burning),
        _ => {}
    }
}
