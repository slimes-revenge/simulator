//! One fight by the game's rules: board, creatures, statuses, strikes, enemy AI, turn order.
//!
//! Each function names the C# method it mirrors (slimes-revenge/SlimesRevenge,
//! `Assets/Scripts/Runtime`). Rules notes: `docs/rules-model.md`.

use crate::data::{ALL_LIQUIDS, Design, Hunter, Liquid, Personality, Trait};
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
    /// D3 (Living enemies): `pus.infected_damage` HP per pulse, bite −`pus.infected_bite`.
    Infected,
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
            StatusKind::Infected => "Infected",
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
    /// A puddle an enemy stepped into (D1).
    Puddle,
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
    /// R2: the creature's element (enemies; `None` for the slime and element-less kinds).
    pub element: Option<Liquid>,
    /// Statuses that never take on it, besides its element's (`[[enemies]] status_immune`).
    pub status_immune: Vec<String>,
    pub traits: Vec<Trait>,
    /// Its own sight range (enemies); the fight's `vision` when `None`.
    pub vision: Option<i32>,
    /// G17: the hunter; it engages once it has seen the slime (`met`) or stands next to it.
    pub hunter: bool,
    pub met: bool,
    /// G4: it has raised its alarm (once per enemy).
    pub alarmed: bool,
    /// Thorns: a melee strike on this creature costs the striker this many edge units.
    pub thorns: usize,
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
            // Dross is cargo, not life: a body of nothing but Dross is dead (balance D4).
            self.volume.iter().any(|&l| l != Liquid::Dross)
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
        // Speed 0 is a stationary creature (design, level 2); otherwise at least 1 like the game.
        let base = if self.base_speed == 0 {
            0
        } else {
            self.base_speed.max(1)
        };
        (base + modifier).max(0)
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

pub fn in_rect(r: Rect, c: Cell) -> bool {
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
    /// Units left in the river at the exit, and its liquids in drinking order (cycling).
    pub river: usize,
    pub river_liquids: Vec<Liquid>,
    /// Water drip cells and the units left on each.
    pub drips: Vec<(Cell, usize)>,
    /// Rooms under the sky and how often (slime rounds) the sun dries the edge-most Water.
    pub surface: Vec<Rect>,
    pub sun_every: u32,
    /// The slime's body when the run ended (carried to the next ladder level).
    pub body: Vec<Liquid>,
    /// G17: the hunter still to enter, then its index in `enemies` once it has.
    pub hunter: Option<Hunter>,
    pub hunter_at: Option<usize>,
    /// The hunter entered and died (set when the run ends).
    pub hunter_dead: bool,
    /// The hunter's bite killed the slime.
    pub hunter_killed_slime: bool,
    /// D6: puddles on the level when the run ended; Pus held as a bad affinity then.
    pub puddles_left: usize,
    pub pus_affinity: bool,
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
    /// The hunter was engaged with the slime during it.
    pub hunter: bool,
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
            ..MapState::default()
        }
    }
}

#[derive(Clone, Debug)]
struct OpenEpisode {
    max_adjacent: usize,
    /// spent, bitten, status, grown when the episode opened.
    counters: (usize, usize),
    alive_at_open: Vec<bool>,
    hunter: bool,
}

/// How the slime chooses its strikes (matrix and stage levels; map levels walk by their own
/// policy and strike like `Newcomer`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Policy {
    /// Strikes with whatever is on the edge; never swaps.
    Newcomer,
    /// Before a strike, at most one swap(0, 1) by the balance session's rules (`arranger_swap`).
    Arranger { keep: Option<Liquid> },
    /// Strike, then step out of reach on the round's last action (`kite_adjacent`).
    Kite,
    /// R3: `kite`, and before a strike it brings a unit that dominates the target's element
    /// from at most 2 places under the edge up to the edge (one action).
    Reader,
    /// D5: `kite` until Pus is a bad affinity; then it farms its burps (`pus_act`).
    Pus,
    /// D7: `kite` until Mercury is its affinity; then it goes for the enemies it sees and
    /// strikes with Mercury while spending it keeps the affinity (`mercury_pick`). It keeps
    /// the body within Mercury / `dominance_percent` (`build_full`).
    Mercury,
    /// D8 `b-<liquid>` (`Some`) and `b-auto` (`None`, the liquid of its affinity or else its
    /// most common one, each turn): `kite`; while the liquid is its affinity it brings the
    /// liquid up against a target it dominates (`build_pick`) and keeps its share
    /// (`build_full`).
    Build(Option<Liquid>),
}

impl Policy {
    pub fn from_name(name: &str, d: &Design) -> Policy {
        match name {
            "arranger" => Policy::Arranger {
                keep: d.policies.keep(),
            },
            "kite" => Policy::Kite,
            "reader" => Policy::Reader,
            "pus" => Policy::Pus,
            "mercury" => Policy::Mercury,
            "b-auto" => Policy::Build(None),
            _ => match name.strip_prefix("b-").and_then(Liquid::from_lower) {
                Some(l) => Policy::Build(Some(l)),
                None => Policy::Newcomer,
            },
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
    /// Spits (strikes without melee contact).
    pub spits: usize,
    /// D3: Pus puddle units burped.
    pub burped: usize,
    /// R2: slime hits that dominated the target's element; suppressed enemy bites that missed.
    pub elem_plus: usize,
    pub elem_miss: usize,
    /// R1: reacting pairs, units they took off the body, stones they dropped.
    pub reactions: usize,
    pub react_loss: usize,
    pub stones: usize,
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
    /// Units the slime lost to Thorns after melee strikes.
    pub units_thorns: usize,
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
    /// The slime's current action is the round's last one (`kite` steps away on it).
    last_action: bool,
    /// Slime turns left in a net (`Nets`): no steps, strikes allowed.
    rooted: u32,
    /// R1 stones: cells and the slime rounds they still stand.
    stones: Vec<(Cell, u32)>,
    /// This slime turn's encouragement bonus is used.
    encouraged: bool,
    /// Enemies `newcomer`/`strafe` already wasted its one same-element strike on (R2 stalls).
    wasted: Vec<usize>,
    /// Matrix fights: no living enemy can be hurt any more (R2 stalls).
    stalemate: bool,
    /// D1 puddles: cell, liquid, units (1–2), one per cell; they last the whole fight.
    pub puddles: Vec<(Cell, Liquid, usize)>,
    /// D3: Pus hits apply Infected (the level's `pus_infects`).
    pub pus_infects: bool,
    round: u32,
    rng: Rng,
    actions_per_round: u32,
    pub slime: Creature,
    pub enemies: Vec<Creature>,
    next_status_id: u32,
    units_spent: usize,
    units_bitten: usize,
    units_status: usize,
    units_thorns: usize,
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
            element: None,
            status_immune: Vec::new(),
            traits: Vec::new(),
            vision: None,
            hunter: false,
            met: false,
            alarmed: false,
            thorns: 0,
            dominant: None,
            aggroed: false,
            pursuit: None,
            pursuit_left: 0,
            flee_cell: None,
        };
        let enemies = enemies
            .iter()
            .map(|&(kind, cell)| enemy_creature(d, kind, cell))
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
            last_action: false,
            rooted: 0,
            stones: Vec::new(),
            encouraged: false,
            wasted: Vec::new(),
            stalemate: false,
            puddles: Vec::new(),
            pus_infects: false,
            round: 0,
            rng: Rng::new(seed),
            actions_per_round,
            slime,
            enemies,
            next_status_id: 0,
            units_spent: 0,
            units_bitten: 0,
            units_status: 0,
            units_thorns: 0,
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
            self.round = round;
            self.note(|_| format!("round {round}"));
            self.encouraged = false;
            // D3: a burp round costs the whole turn.
            let actions = if self.burps(round) {
                self.burp();
                0
            } else {
                self.action_points()
            };
            for k in 0..actions {
                self.last_action = k + 1 == actions;
                self.slime_act();
                self.track_engagement();
                if self.stalemate {
                    self.last_harm = Some("stalemate".into());
                    return self.result(false, round);
                }
                // Thorns can kill the slime on its own strike.
                if !self.slime.alive() {
                    return self.result(false, round);
                }
                if self.goal_reached() {
                    return self.result(true, round);
                }
            }
            self.rooted = self.rooted.saturating_sub(1);
            self.react();
            self.fester();
            self.sun(round);
            self.track_engagement();
            if !self.slime.alive() {
                return self.result(false, round);
            }
            self.enter_hunter(round);
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

    /// This round's action points: the base, minus 1 per heavy liquid held at its count
    /// (`rules.heavy`), never below `rules.heavy_min_action_points`, or the base when lower.
    fn action_points(&self) -> u32 {
        let r = &self.d.rules;
        let heavy = r
            .heavy
            .iter()
            .filter(|&(&l, &n)| self.slime.count(l) >= n)
            .count() as u32;
        let floor = r.heavy_min_action_points.min(self.actions_per_round);
        self.actions_per_round.saturating_sub(heavy).max(floor)
    }

    /// The sun: after slime round `round`, in a surface room, every `sun_every` rounds, it
    /// dries the edge-most Water. Taking the last unit kills.
    fn sun(&mut self, round: u32) {
        let Some(m) = self.map.as_ref() else { return };
        let cell = self.slime.cell;
        if m.sun_every == 0
            || !round.is_multiple_of(m.sun_every)
            || !m.surface.iter().any(|&r| in_rect(r, cell))
        {
            return;
        }
        let surface = m.surface.clone();
        for p in &mut self.puddles {
            if p.1 == Liquid::Water && surface.iter().any(|&r| in_rect(r, p.0)) {
                p.2 -= 1;
            }
        }
        self.puddles.retain(|p| p.2 > 0);
        let Some(i) = self.slime.volume.iter().rposition(|&l| l == Liquid::Water) else {
            return;
        };
        self.slime.volume.remove(i);
        self.last_harm = Some("sun".into());
        let d = self.d;
        refresh_dominance(d, &mut self.slime, &mut self.next_status_id);
        self.note(|_| "The sun dries a Water unit".into());
    }

    /// What `drink` would take where the slime stands, if anything.
    fn next_drink(&self) -> Option<Liquid> {
        let cell = self.slime.cell;
        let m = self.map.as_ref()?;
        if self.slime.volume.len() >= self.d.slime.capacity {
            return None;
        }
        if m.drips.iter().any(|(c, n)| *c == cell && *n > 0) {
            Some(Liquid::Water)
        } else if cell == m.exit && m.river > 0 {
            m.river_liquids.first().copied()
        } else {
            None
        }
    }

    /// Drinks one unit where the slime stands: a drip cell's Water, or the river at the exit.
    /// `false` when full or nothing is here. `keep = false` pours the unit away instead (R3).
    fn drink(&mut self, keep: bool) -> bool {
        let cell = self.slime.cell;
        let full = self.slime.volume.len() >= self.d.slime.capacity;
        // D5: `pus` never drinks itself out of its bad affinity; at the exit it leaves the rest
        // of the river.
        let n = self.slime.volume.len() + 1;
        if self.policy == Policy::Pus
            && self.pus_affinity()
            && self.next_drink().is_some_and(|l| l != Liquid::Pus)
            && self.slime.count(Liquid::Pus) * 100 < n * self.d.pus.affinity_percent
        {
            if let Some(m) = self.map.as_mut().filter(|m| m.exit == cell) {
                m.river = 0;
                self.note(|_| "Slime leaves the river: its Pus would thin".into());
            }
            return false;
        }
        // D7, D8: `mercury` and the builds drink nothing that thins their liquid past its share.
        if self.next_drink().is_some_and(|l| self.build_full(l)) {
            if let Some(m) = self.map.as_mut().filter(|m| m.exit == cell) {
                m.river = 0;
                self.note(|f| {
                    let l = f.build_liquid().expect("a build");
                    format!("Slime leaves the river: its {l:?} would thin")
                });
            }
            return false;
        }
        let Some(m) = self.map.as_mut().filter(|_| !full) else {
            return false;
        };
        let liquid = if let Some(drip) = m.drips.iter_mut().find(|(c, n)| *c == cell && *n > 0) {
            drip.1 -= 1;
            Liquid::Water
        } else if cell == m.exit && m.river > 0 {
            m.river -= 1;
            m.river_liquids.rotate_left(1);
            *m.river_liquids.last().expect("river_liquids go with river")
        } else {
            return false;
        };
        if !keep {
            self.note(|_| format!("Slime pours {liquid:?} away: it would react"));
            return true;
        }
        self.slime.volume.push(liquid);
        let d = self.d;
        refresh_dominance(d, &mut self.slime, &mut self.next_status_id);
        self.note(|_| format!("Slime drinks {liquid:?}"));
        true
    }

    /// Matrix fights end when every enemy is dead; map runs when the slime stands on the exit
    /// and has drunk the river (full or empty).
    fn goal_reached(&self) -> bool {
        match &self.map {
            Some(m) => {
                self.slime.alive()
                    && self.slime.cell == m.exit
                    && (m.river == 0 || self.slime.volume.len() >= self.d.slime.capacity)
            }
            None => self.slime.alive() && self.enemies_dead(),
        }
    }

    /// Map runs only: opens an episode when an enemy is aggroed on the slime, tracks how many
    /// aggroed enemies stand next to it at once, closes it when none is left.
    fn track_engagement(&mut self) {
        let Some(m) = self.map.as_ref() else { return };
        let engaged = |e: &Creature| self.engaging(e);
        let aggro = self.enemies.iter().any(engaged);
        let adjacent = self
            .enemies
            .iter()
            .filter(|e| engaged(e) && chebyshev(e.cell, self.slime.cell) == 1)
            .count();
        let counters = (self.units_lost(), self.units_grown);
        let alive: Vec<bool> = self.enemies.iter().map(|e| e.alive()).collect();
        let hunter = m.hunter_at.is_some_and(|h| engaged(&self.enemies[h]));
        let open = m.open.is_some();
        let m = self.map.as_mut().expect("checked");
        let now = OpenEpisode {
            max_adjacent: adjacent,
            counters,
            alive_at_open: alive,
            hunter: false,
        };
        if let Some(o) = m.open.as_mut() {
            o.max_adjacent = o.max_adjacent.max(adjacent);
            o.hunter |= hunter;
        } else if aggro {
            let calm = m.calm.take().unwrap_or_else(|| now.clone());
            m.open = Some(OpenEpisode {
                max_adjacent: adjacent,
                hunter,
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
        let (lost, grown) = o.counters;
        let mut ep = Episode {
            max_adjacent: o.max_adjacent,
            hunter: o.hunter,
            ..Episode::default()
        };
        if self.slime.alive() {
            let mut corpses = Vec::new();
            for (i, e) in self.enemies.iter().enumerate() {
                // An enemy that entered after the snapshot (the hunter) was alive at the open.
                let was_alive = o.alive_at_open.get(i).copied().unwrap_or(true);
                if was_alive && !e.alive() {
                    let mut corpse = e.volume.clone();
                    // D8: a Living enemy killed while Poisoned leaves half (rounded up) Pus.
                    let poisoned = e.statuses.iter().any(|s| s.kind == StatusKind::Poisoned);
                    if poisoned && e.element.is_some() && e.element == self.d.elements.living {
                        let half = corpse.len().div_ceil(2);
                        corpse[..half].fill(Liquid::Pus);
                    }
                    corpses.extend(corpse);
                }
            }
            // D7, D8: `mercury` and `b-<liquid>` eat their liquid first and leave what would
            // thin it; `b-auto` eats anything.
            let picky = self.policy != Policy::Build(None);
            if let Some(l) = self.build_liquid().filter(|_| picky) {
                corpses.sort_by_key(|&u| u != l);
            }
            for unit in corpses {
                ep.corpse += 1;
                let thins = picky && self.build_full(unit);
                if self.slime.volume.len() < self.d.slime.capacity && !thins {
                    self.slime.volume.push(unit);
                    ep.eaten += 1;
                } else {
                    ep.wasted += 1;
                }
            }
            let d = self.d;
            refresh_dominance(d, &mut self.slime, &mut self.next_status_id);
        }
        ep.net = (ep.corpse + self.units_grown - grown) as i64 - (self.units_lost() - lost) as i64;
        let body = self.slime.volume.len();
        let m = self.map.as_mut().expect("an episode was open");
        m.min_body = m.min_body.min(body);
        m.episodes.push(ep);
    }

    /// Plays a map run and returns its map state (episodes, rooms entered) as well.
    pub fn run_map(mut self) -> (FightResult, MapState) {
        self.track_engagement();
        let mut result = self.rounds();
        self.close_episode();
        // Units that met at the exit (the last turn, eaten corpses) react before the body
        // leaves the level.
        if self.slime.alive() {
            self.react();
            result.units_left = self.slime.volume.len();
            result.tally = self.tally.clone();
            if !self.slime.alive() {
                result.won = false;
                result.cause = Some("reaction".into());
            }
        }
        let mut state = self.map.take().unwrap_or_default();
        state.body = self.slime.volume.clone();
        state.puddles_left = self.puddles.len();
        state.pus_affinity = self.pus_affinity();
        state.hunter_dead = state.hunter_at.is_some_and(|h| !self.enemies[h].alive());
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
            units_thorns: self.units_thorns,
            units_grown: self.units_grown,
            tally: self.tally.clone(),
            enemies_killed: self.enemies.iter().filter(|e| !e.alive()).count(),
            cause: if won { None } else { self.last_harm.clone() },
        }
    }

    /// An enemy in the fight with the slime: aggroed and able to come (speed above 0) or
    /// already next to it. A stationary blocker that saw the slime does not hold it.
    /// The hunter always knows where the slime is, so it engages only from the first time it
    /// sees the slime (its sight range, walls block) until it dies, or while next to it.
    fn engaging(&self, e: &Creature) -> bool {
        let adjacent = chebyshev(e.cell, self.slime.cell) == 1;
        let comes = if e.hunter { e.met } else { e.base_speed > 0 };
        e.alive() && e.aggroed && (comes || adjacent)
    }

    /// An enemy's sight range: its own or the fight's, +2 for `SmellsBlood` while the slime's
    /// edge unit is Blood (G2).
    fn sight(&self, e: &Creature) -> i32 {
        let smells = e.traits.contains(&Trait::SmellsBlood)
            && self.slime.volume.last() == Some(&Liquid::Blood);
        e.vision.unwrap_or(self.vision) + if smells { 2 } else { 0 }
    }

    /// Every unit the slime has lost so far: spent, bitten, to statuses, to thorns.
    fn units_lost(&self) -> usize {
        self.units_spent
            + self.units_bitten
            + self.units_status
            + self.units_thorns
            + self.tally.react_loss
    }

    fn enemies_dead(&self) -> bool {
        self.enemies.iter().all(|e| !e.alive())
    }

    /// Cells a walker cannot enter: living creatures and reaction stones.
    fn occupied(&self) -> Vec<Cell> {
        std::iter::once(&self.slime)
            .chain(&self.enemies)
            .filter(|c| c.alive())
            .map(|c| c.cell)
            .chain(self.stones.iter().map(|s| s.0))
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
        let living: Vec<usize> = (0..self.enemies.len())
            .filter(|&i| self.enemies[i].alive())
            .collect();
        let doomed = |i: usize| {
            let e = &self.enemies[i];
            pending_status_damage(self.d, e) >= e.hit_points
        };
        if !living.is_empty() && living.iter().all(|&i| self.gives_up(i) && !doomed(i)) {
            self.stalemate = true;
            return self.note(|_| "Slime cannot hurt anyone left: a stalemate".into());
        }
        if self.hunts() {
            if !self.mercury_hunt() {
                self.note(|_| "Slime waits: no enemy in reach".into());
            }
            return;
        }
        if self.kites() {
            if !self.enemies.iter().any(|e| self.engaging(e)) {
                return self.note(|_| "Slime waits for the enemy".into());
            }
            if self.kite_adjacent() {
                return;
            }
            // Only harmless neighbours (no bite, e.g. a blocker): hit them; else wait for them.
            let harmless = (0..self.enemies.len()).find(|&i| {
                let e = &self.enemies[i];
                self.engaging(e) && chebyshev(e.cell, self.slime.cell) == 1 && !self.gives_up(i)
            });
            match harmless {
                Some(i) => self.strike_or_wait(i),
                None => self.note(|_| "Slime waits for the enemy".into()),
            }
            return;
        }
        let Some(target) = self
            .enemies
            .iter()
            .enumerate()
            .filter(|&(i, e)| e.alive() && !self.gives_up(i))
            .min_by_key(|(i, e)| (chebyshev(self.slime.cell, e.cell), *i))
            .map(|(i, _)| i)
        else {
            return;
        };
        if chebyshev(self.slime.cell, self.enemies[target].cell) == 1 {
            if self.unstall(target) {
                return;
            }
            // `Volume.CanSpend`: the last unit is the slime's life.
            if self.can_strike()
                && !self.swapped
                && self.arranger_swap(target)
                && !self.would_react(self.slime.volume.len() - 1, self.slime.volume.len() - 2)
                && !self.useless_against(self.slime.volume[self.slime.volume.len() - 2], target)
            {
                self.slime_swap(0);
                self.swapped = true;
                self.note(|f| format!("Slime swaps: {:?} to the edge", f.edge()));
            } else if self.can_strike() && self.edge() == Liquid::Pus {
                self.swapped = false;
                self.slime_spit(target);
                self.note(|_| "Slime spits Pus off its edge".into());
            } else if self.can_strike() {
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
        // A pending swap belongs to the strike next to it: walking off (a new target) drops it.
        self.swapped = false;
        let from = self.slime.cell;
        if let Some(to) = self.walk_toward(from, self.enemies[target].cell, 1) {
            if self.move_slime(to) {
                self.note(|_| format!("Slime steps {from:?}->{to:?}"));
            }
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
        if self.policy == Policy::Pus && self.pus_affinity() && self.pus_act() {
            return;
        }
        // R2 stalls: an enemy the slime gives up on is neither struck nor waited for.
        let adjacent = |f: &Fight, aggroed_only: bool| {
            (0..f.enemies.len()).find(|&i| {
                let e = &f.enemies[i];
                e.alive()
                    && chebyshev(e.cell, f.slime.cell) == 1
                    && (e.aggroed || !aggroed_only)
                    && !f.gives_up(i)
            })
        };
        let aggro = (0..self.enemies.len())
            .any(|i| self.engaging(&self.enemies[i]) && !self.gives_up(i) && self.can_reach(i));
        if self.kites() && aggro && self.kite_adjacent() {
            return;
        }
        if let Some(i) = adjacent(self, true) {
            return self.strike_or_wait(i);
        }
        // R3: careful map policies (`lure`, `kite`, `reader`; not `strafe`) spit off an edge
        // unit that would react with the one under it, and pour away (one action) a unit
        // they would drink onto an edge it reacts with.
        let careful = self.map.as_ref().is_some_and(|m| m.lure);
        let v = &self.slime.volume;
        if careful && v.len() >= 2 && self.pair_reacts(v[v.len() - 1], v[v.len() - 2]) {
            let unit = self.slime.volume.pop().expect("two units");
            self.units_spent += 1;
            *self.tally.spent_by.entry(unit).or_insert(0) += 1;
            return self.note(|_| format!("Slime spits {unit:?} away: it would react"));
        }
        let reacts = self.next_drink().is_some_and(|l| {
            self.slime
                .volume
                .last()
                .is_some_and(|&e| self.pair_reacts(e, l))
        });
        if self.drink(!(careful && reacts)) {
            return;
        }
        if self.hunts() && self.mercury_hunt() {
            return;
        }
        let m = self.map.as_ref().expect("map runs");
        let (lure, exit, waited) = (m.lure, m.exit, m.waited);
        let goal = if lure && aggro {
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
            if self.move_slime(to) {
                self.note(|_| format!("Slime steps {from:?}->{to:?}"));
            }
        } else {
            // Blocked: follow the route through creatures; step if the next cell is free,
            // else strike (or spit at) whoever stands in it or next to the slime.
            let through = path::find(
                self.board.width,
                self.board.height,
                self.slime.cell,
                &[goal],
                |c| self.board.wall(c),
            );
            let next = through.map(|r| r[0]);
            let blocker =
                next.and_then(|c| self.enemies.iter().position(|e| e.alive() && e.cell == c));
            if let Some(i) = blocker.filter(|&i| self.gives_up(i)) {
                // A blocker it cannot hurt: wait for it to move (or die of its statuses). The
                // run ends only when stationary given-up enemies close every way to the goal.
                let stuck = |e: &Creature| {
                    e.alive()
                        && e.base_speed == 0
                        && pending_status_damage(self.d, e) < e.hit_points
                };
                let walls: Vec<Cell> = (0..self.enemies.len())
                    .filter(|&j| stuck(&self.enemies[j]) && self.gives_up(j))
                    .map(|j| self.enemies[j].cell)
                    .collect();
                let way = path::find(
                    self.board.width,
                    self.board.height,
                    self.slime.cell,
                    &[goal],
                    |c| self.board.wall(c) || walls.contains(&c),
                );
                if way.is_none() {
                    self.stalemate = true;
                    return self.note(|f| format!("{} blocks the way for good", f.label(i)));
                }
                self.note(|f| format!("Slime waits: {} blocks the way", f.label(i)));
            } else if let Some(i) = blocker.or_else(|| adjacent(self, true)) {
                self.strike_or_wait(i);
            } else if let Some(to) = next.filter(|&c| !self.occupied().contains(&c)) {
                let from = self.slime.cell;
                if self.move_slime(to) {
                    self.note(|_| format!("Slime steps {from:?}->{to:?}"));
                }
            } else {
                self.note(|_| "Slime waits: no path to the exit".into());
            }
        }
    }

    /// Moves the slime one cell; on a map, entering a room counts it and resets lure's wait.
    /// `false` when the slime is netted and stays.
    fn move_slime(&mut self, to: Cell) -> bool {
        if self.rooted > 0 {
            self.note(|_| "Slime is netted: it cannot step".into());
            return false;
        }
        self.slime.cell = to;
        // A pending swap or bring-up belongs to the strike next to it.
        self.swapped = false;
        if let Some(m) = self.map.as_mut()
            && let Some(k) = m.rooms.iter().position(|&r| in_rect(r, to))
        {
            m.rooms_entered = m.rooms_entered.max(k + 1);
            m.waited = 0;
        }
        true
    }

    /// How far `c` is from the nearest room's centre (Chebyshev); 0 without rooms.
    fn interior_distance(&self, c: Cell) -> i32 {
        self.map.as_ref().map_or(0, |m| {
            m.rooms
                .iter()
                .map(|&((x0, y0), (x1, y1))| chebyshev(c, ((x0 + x1) / 2, (y0 + y1) / 2)))
                .min()
                .unwrap_or(0)
        })
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
        let hidden = self.invisible();
        self.enemies
            .iter()
            .filter(|e| e.alive() && !e.aggroed && e.personality == Personality::Aggressive)
            .filter(|e| e.base_speed > 0)
            .filter(|e| !hidden && chebyshev(e.cell, cell) <= self.sight(e))
            .filter(|e| self.board.line_of_sight(e.cell, cell))
            .count()
    }

    /// Where `lure` fights: the nearest non-room cell, preferring corridor cells with at most
    /// two open neighbours (one enemy can reach it from each side) over doorways; ties go west.
    fn fighting_spot(&self) -> Option<Cell> {
        let occupied = self.occupied();
        // One search from the slime gives the route length to every cell.
        let tree = path::Tree::new(self.board.width, self.board.height, self.slime.cell, |c| {
            self.blocked(&occupied, c)
        });
        let mut best: Option<((bool, i32, i32), Cell)> = None;
        for y in 0..self.board.height {
            for x in 0..self.board.width {
                let c = (x, y);
                if !self.board.open(c) || self.in_room(c) {
                    continue;
                }
                let Some(steps) = tree.steps_to(c) else {
                    continue;
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

    /// `Volume.CanSpend`: the last unit is the slime's life. With `rules.last_drop_strike` it
    /// may still strike: the last drop hits and is not spent.
    fn can_strike(&self) -> bool {
        self.slime.volume.len() > 1
            || (self.d.rules.last_drop_strike && !self.slime.volume.is_empty())
    }

    /// `kite` with enemies next to it: strike the weakest (lowest HP, then placement order),
    /// except on the round's last action, when it steps to a free cell no living enemy
    /// touches. A step with no such cell falls back to a strike, and a strike that cannot be
    /// spent falls back to a step. Returns false when no living enemy is adjacent.
    fn kite_adjacent(&mut self) -> bool {
        let Some(target) = self
            .enemies
            .iter()
            .enumerate()
            .filter(|&(i, e)| {
                e.alive()
                    && e.melee_power > 0
                    && chebyshev(e.cell, self.slime.cell) == 1
                    && !self.gives_up(i)
                    // `pus` never wastes Pus on the Living before `pus_infects` levels.
                    && !(self.policy == Policy::Pus && !self.pus_infects && self.living(i))
            })
            .min_by_key(|(i, e)| (e.hit_points, *i))
            .map(|(i, _)| i)
        else {
            return false;
        };
        // Netted: it cannot step back, so it strikes.
        if (self.last_action || !self.can_strike())
            && self.rooted == 0
            && let Some(to) = self.kite_step()
        {
            let from = self.slime.cell;
            // A pending bring-up belongs to the strike it preceded.
            self.swapped = false;
            if self.move_slime(to) {
                self.note(|_| format!("Slime steps back {from:?}->{to:?}"));
            }
            return true;
        }
        self.strike_or_wait(target);
        true
    }

    /// Where `kite` steps back: a free neighbour no living enemy is adjacent to. The largest
    /// smallest distance to the enemies wins, then the cell farther from the nearest room's
    /// centre, then the lowest (x, y).
    fn kite_step(&self) -> Option<Cell> {
        let occupied = self.occupied();
        let living: Vec<Cell> = self
            .enemies
            .iter()
            .filter(|e| e.alive() && e.melee_power > 0)
            .map(|e| e.cell)
            .collect();
        path::neighbours(self.slime.cell)
            .filter(|&c| self.inside(c) && !occupied.contains(&c))
            .map(|c| {
                (
                    living
                        .iter()
                        .map(|&e| chebyshev(e, c))
                        .min()
                        .unwrap_or(i32::MAX),
                    c,
                )
            })
            .filter(|&(near, _)| near >= 2)
            .min_by_key(|&(near, c)| (-near, -self.interior_distance(c), c.0, c.1))
            .map(|(_, c)| c)
    }

    /// Strikes `target` with the edge unit, or spits it when the policy spits at Thorns
    /// (`lure`, `kite`) and the target has them; waits when the last unit cannot be spent.
    fn strike_or_wait(&mut self, target: usize) {
        if self.unstall(target) {
            return;
        }
        if self.policy == Policy::Reader
            && self.can_strike()
            && !self.swapped
            && let Some(k) = self.reader_pick(target)
        {
            let unit = self.slime.volume.remove(k);
            self.slime.volume.push(unit);
            self.tally.swaps += 1;
            self.swapped = true;
            return self.note(|_| format!("Slime brings {unit:?} to the edge"));
        }
        if self.can_strike()
            && !self.swapped
            && let Some(k) = self
                .mercury_pick(target)
                .or_else(|| self.build_pick(target))
        {
            let unit = self.slime.volume.remove(k);
            self.slime.volume.push(unit);
            self.tally.swaps += 1;
            self.swapped = true;
            return self.note(|_| format!("Slime brings {unit:?} to the edge"));
        }
        self.swapped = false;
        let spits = self.kites() || self.map.as_ref().is_some_and(|m| m.lure);
        if self.can_strike() && self.edge() == Liquid::Pus {
            self.slime_spit(target);
            self.note(|_| "Slime spits Pus off its edge".into());
        } else if self.can_strike() && spits && self.enemies[target].thorns > 0 {
            let liquid = self.edge();
            self.slime_spit(target);
            self.note(|f| format!("Slime spits at {} with {liquid:?}", f.label(target)));
        } else if self.can_strike() {
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
        // A swap would put the edge unit next to Pus (unit 2).
        let beside_pus = n >= 3 && self.slime.volume[n - 3] == Liquid::Pus;
        let has =
            |k: fn(&StatusKind) -> bool| self.enemies[target].statuses.iter().any(|s| k(&s.kind));
        (keep == Some(edge) && next != edge && next != Liquid::Pus && !beside_pus)
            || (edge == Liquid::Water
                && next != Liquid::Water
                && has(|k| matches!(k, StatusKind::Burning | StatusKind::Corroding { .. })))
            || (next == Liquid::Lava
                && edge != Liquid::Lava
                && has(|k| *k == StatusKind::Flammable))
    }

    /// Whether enemy `i` has a way to the slime past walls, stones and the enemies the slime
    /// gave up on (they never clear the way; other creatures move).
    fn can_reach(&self, i: usize) -> bool {
        let e = self.enemies[i].cell;
        if chebyshev(e, self.slime.cell) <= 1 {
            return true;
        }
        let walls: Vec<Cell> = (0..self.enemies.len())
            .filter(|&j| j != i && self.enemies[j].alive() && self.gives_up(j))
            .map(|j| self.enemies[j].cell)
            .chain(self.stones.iter().map(|s| s.0))
            .collect();
        path::find(
            self.board.width,
            self.board.height,
            e,
            &[self.slime.cell],
            |c| self.board.wall(c) || walls.contains(&c),
        )
        .is_some()
    }

    /// R2 stalls: the edge unit is enemy `i`'s own element, so a strike would do nothing.
    fn edge_useless(&self, i: usize) -> bool {
        self.slime
            .volume
            .last()
            .is_some_and(|&l| self.useless_against(l, i))
    }

    /// `unit` is enemy `i`'s own element (R2).
    fn useless_against(&self, unit: Liquid, i: usize) -> bool {
        let theirs = self.enemies[i].element;
        theirs.is_some() && self.d.elements.of(unit) == theirs
    }

    /// A unit 1 or 2 under the edge that is not enemy `i`'s element, when bringing it up
    /// makes no reacting neighbours.
    fn unstall_pick(&self, i: usize) -> Option<usize> {
        let theirs = self.enemies[i].element;
        self.bring_up(|unit| self.d.elements.of(unit) != theirs)
    }

    /// Policies other than `newcomer`/`strafe` avoid waste; those two waste one strike first.
    fn careful(&self) -> bool {
        self.policy != Policy::Newcomer || self.map.as_ref().is_some_and(|m| m.lure)
    }

    /// The slime gives up on enemy `i`: its edge cannot hurt it, no unit within 2 of the edge
    /// can, and (for `newcomer`/`strafe`) its one wasted strike is spent.
    fn gives_up(&self, i: usize) -> bool {
        self.edge_useless(i)
            && self.unstall_pick(i).is_none()
            && (self.careful() || self.wasted.contains(&i))
    }

    /// Before a strike on `i` with a useless edge: `newcomer`/`strafe` waste one strike first;
    /// then any policy brings a unit that can hurt it to the edge (one action). `true` when
    /// it acted.
    fn unstall(&mut self, i: usize) -> bool {
        if !self.edge_useless(i) || !self.can_strike() {
            return false;
        }
        if !self.careful() && !self.wasted.contains(&i) {
            self.wasted.push(i);
            return false;
        }
        let Some(k) = self.unstall_pick(i) else {
            return false;
        };
        let unit = self.slime.volume.remove(k);
        self.slime.volume.push(unit);
        self.tally.swaps += 1;
        self.note(|_| format!("Slime brings {unit:?} up: its edge cannot hurt its own element"));
        true
    }

    /// The index (1 or 2 under the edge, nearest first) of a unit `wanted` accepts whose move to
    /// the edge makes no more reacting neighbours (R3): the one-action bring-up of the stall
    /// rule, `reader`, `mercury` and the builds.
    fn bring_up(&self, wanted: impl Fn(Liquid) -> bool) -> Option<usize> {
        let v = &self.slime.volume;
        let n = v.len();
        (n.saturating_sub(3)..n.saturating_sub(1)).rev().find(|&k| {
            let mut after = v.clone();
            let unit = after.remove(k);
            after.push(unit);
            wanted(unit) && self.reacting_pairs(&after) <= self.reacting_pairs(v)
        })
    }

    /// `kite`, `reader`, `pus`, the builds, and `mercury` without its affinity.
    fn kites(&self) -> bool {
        matches!(
            self.policy,
            Policy::Kite | Policy::Reader | Policy::Pus | Policy::Build(_)
        ) || (self.policy == Policy::Mercury && !self.hunts())
    }

    /// D7, D8: the liquid `mercury` or a build plays: Mercury, its own, or for `b-auto` the
    /// liquid of its affinity, else its most common one (the first in `ALL_LIQUIDS` on a tie).
    fn build_liquid(&self) -> Option<Liquid> {
        match self.policy {
            Policy::Mercury => Some(Liquid::Mercury),
            Policy::Build(Some(l)) => Some(l),
            Policy::Build(None) => self.slime.dominant.or_else(|| {
                ALL_LIQUIDS
                    .iter()
                    .copied()
                    .filter(|&l| l != Liquid::Dross && self.slime.count(l) > 0)
                    .max_by_key(|&l| (self.slime.count(l), std::cmp::Reverse(l)))
            }),
            _ => None,
        }
    }

    /// D7: `mercury` holding its affinity goes for the enemies instead of kiting.
    fn hunts(&self) -> bool {
        self.policy == Policy::Mercury && self.slime.dominant == Some(Liquid::Mercury)
    }

    /// D7: the Mercury affinity hides the slime (`rules.mercury_affinity = "invisible"`, the
    /// game) instead of defending it.
    fn invisible(&self) -> bool {
        self.slime.dominant == Some(Liquid::Mercury) && self.d.rules.mercury_affinity == "invisible"
    }

    /// D7, D8: the played liquid is the affinity and taking `unit` would take the body past
    /// its count × 100 / `dominance_percent`.
    fn build_full(&self, unit: Liquid) -> bool {
        let Some(l) = self.build_liquid().filter(|&l| l != unit) else {
            return false;
        };
        self.slime.dominant == Some(l)
            && (self.slime.volume.len() + 1) * self.d.rules.dominance_percent
                > self.slime.count(l) * 100
    }

    /// D8: a build holding its liquid as the affinity: the index (1 or 2 under the edge) of a
    /// unit of it to bring up before a strike on a target it dominates, when the edge is not
    /// one and the move makes no reacting neighbours.
    fn build_pick(&self, target: usize) -> Option<usize> {
        let l = self.build_liquid()?;
        let el = &self.d.elements;
        let theirs = self.enemies[target].element?;
        let v = &self.slime.volume;
        let n = v.len();
        let dominates = el.of(l).is_some_and(|a| el.suppresses(a, theirs));
        if !matches!(self.policy, Policy::Build(_))
            || self.slime.dominant != Some(l)
            || !dominates
            || n < 2
            || v[n - 1] == l
        {
            return None;
        }
        self.bring_up(|unit| unit == l)
    }

    /// D7 `mercury` with its affinity: the index (1 or 2 under the edge) of the unit to bring up
    /// before a strike on `target`. Mercury while spending one keeps the affinity and the target
    /// is not of Mercury's element; else a non-Mercury unit. `None` when the edge already is
    /// one, or no such unit is near, or the move makes reacting neighbours.
    fn mercury_pick(&self, target: usize) -> Option<usize> {
        if !self.hunts() {
            return None;
        }
        let v = &self.slime.volume;
        let n = v.len();
        if n < 2 {
            return None;
        }
        let m = self.slime.count(Liquid::Mercury);
        let basis = if self.d.rules.dominance_basis == "capacity" {
            self.d.slime.capacity
        } else {
            n - 1
        };
        let mercury = !self.useless_against(Liquid::Mercury, target)
            && (m - 1) * 100 >= basis * self.d.rules.dominance_percent;
        let wanted = |l: Liquid| (l == Liquid::Mercury) == mercury;
        if wanted(v[n - 1]) {
            return None;
        }
        self.bring_up(wanted)
    }

    /// D7 `mercury` hunting: the nearest living enemy it sees (sight range, walls block) and
    /// can reach and hurt.
    fn mercury_target(&self) -> Option<usize> {
        (0..self.enemies.len())
            .filter(|&i| {
                let e = &self.enemies[i];
                e.alive()
                    && chebyshev(e.cell, self.slime.cell) <= self.vision
                    && self.board.line_of_sight(e.cell, self.slime.cell)
                    && !self.gives_up(i)
                    && self.can_reach(i)
            })
            .min_by_key(|&i| (chebyshev(self.enemies[i].cell, self.slime.cell), i))
    }

    /// D7 `mercury` hunting: strike the target next to it, else step toward it. `false` when
    /// it has no target.
    fn mercury_hunt(&mut self) -> bool {
        let Some(i) = self.mercury_target() else {
            return false;
        };
        if chebyshev(self.enemies[i].cell, self.slime.cell) == 1 {
            self.strike_or_wait(i);
            return true;
        }
        // A pending bring-up belongs to the strike next to it: walking off drops it.
        self.swapped = false;
        let from = self.slime.cell;
        match self.walk_toward(from, self.enemies[i].cell, 1) {
            Some(to) => {
                if self.move_slime(to) {
                    self.note(|f| format!("Slime goes for {}: {from:?}->{to:?}", f.label(i)));
                }
                true
            }
            None => false,
        }
    }

    /// `reader`: the index (1 or 2 under the edge) of a unit whose element dominates the
    /// target's, when the edge does not already and the move makes no reacting neighbours.
    fn reader_pick(&self, target: usize) -> Option<usize> {
        let el = &self.d.elements;
        let theirs = self.enemies[target].element?;
        let v = &self.slime.volume;
        let n = v.len();
        let dominates = |l: Liquid| el.of(l).is_some_and(|a| el.suppresses(a, theirs));
        if n < 2 || dominates(v[n - 1]) {
            return None;
        }
        self.bring_up(dominates)
    }

    fn reacting_pairs(&self, body: &[Liquid]) -> usize {
        body.windows(2)
            .filter(|w| self.pair_reacts(w[0], w[1]))
            .count()
    }

    fn pair_reacts(&self, a: Liquid, b: Liquid) -> bool {
        self.d
            .reactions
            .iter()
            .any(|r| r.pair == (a, b) || r.pair == (b, a))
    }

    /// Whether swapping the units at body indices `i` and `j` makes reacting neighbours.
    fn would_react(&self, i: usize, j: usize) -> bool {
        let mut after = self.slime.volume.clone();
        after.swap(i, j);
        self.reacting_pairs(&after) > self.reacting_pairs(&self.slime.volume)
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

    /// A melee strike: `hit`, then the target's Thorns take that many units off the edge.
    pub fn slime_strike(&mut self, target: usize, liquid: Liquid) {
        let el = &self.d.elements;
        if el.of(liquid).is_some() && el.of(liquid) == self.enemies[target].element {
            // Same element (R2): nothing happens, Thorns included.
            return self.hit(target, liquid);
        }
        self.hit(target, liquid);
        let thorns = self.enemies[target].thorns;
        if thorns > 0 && self.slime.alive() {
            let d = self.d;
            let before = self.slime.volume.len();
            damage(
                d,
                &mut self.slime,
                thorns as i32,
                false,
                &mut self.next_status_id,
            );
            self.units_thorns += before - self.slime.volume.len();
            self.last_harm = Some(format!("thorns:{}", self.enemies[target].name));
        }
    }

    /// Spit (`rules.spit_range`): the edge unit hits like a strike, without melee contact, so
    /// Thorns do not answer. One action.
    pub fn slime_spit(&mut self, target: usize) {
        let liquid = self.edge();
        self.hit(target, liquid);
        self.tally.spits += 1;
    }

    /// `TurnManager.TryAttack` → `Combat.Attack(attacker, target, substance)`.
    fn hit(&mut self, target: usize, liquid: Liquid) {
        let d = self.d;
        // The unit of that kind nearest the edge: the edge unit itself in every policy.
        let Some(at) = self.slime.volume.iter().rposition(|&l| l == liquid) else {
            return;
        };
        // The last drop strikes without being spent, and only under `rules.last_drop_strike`.
        if self.slime.volume.len() == 1 && !d.rules.last_drop_strike {
            return;
        }
        // R2: the edge unit's element against the target's.
        let el = &d.elements;
        let (mine, theirs) = (el.of(liquid), self.enemies[target].element);
        let dominates = mine.zip(theirs).is_some_and(|(a, b)| el.suppresses(a, b));
        let suppressed = mine.zip(theirs).is_some_and(|(a, b)| el.suppresses(b, a));
        if mine.is_some() && mine == theirs {
            // Same element: no harm, no effect, the unit is kept.
            self.enemies[target].aggroed = true;
            return self.note(|f| format!("{} shrugs off its own element", f.label(target)));
        }
        let n = self.slime.volume.len();
        let bonus = if !self.encouraged
            && !suppressed
            && at + 1 == n
            && n >= 2
            && d.elements.encourages(self.slime.volume[n - 2], liquid)
        {
            self.encouraged = true;
            d.elements.encouragement_bonus
        } else {
            0
        };
        if self.slime.volume.len() > 1 {
            self.slime.volume.remove(at);
            self.units_spent += 1;
            *self.tally.spent_by.entry(liquid).or_insert(0) += 1;
        }
        refresh_dominance(d, &mut self.slime, &mut self.next_status_id);

        let stats = d.liquid(liquid);
        let enemy = &mut self.enemies[target];
        if pending_status_damage(d, enemy) >= enemy.hit_points {
            self.tally.doomed_strikes += 1;
        }
        if liquid == Liquid::Ambrosia && enemy.element.is_some() && enemy.element == el.living {
            // Ambrosia heals the Living (lore forum.md) instead of hurting them.
            heal(enemy, 1);
            enemy.aggroed = true;
            return;
        }
        enemy.armor = (enemy.armor - stats.corrosion).max(0);
        let mut power = if liquid == Liquid::Lava {
            modify_incoming_harm(d, enemy, stats.power)
        } else {
            stats.power
        };
        if dominates {
            power = if el.suppress == "double" {
                power * 2
            } else {
                power + 1
            };
            self.tally.elem_plus += 1;
        }
        let power = power + bonus;
        let hp = enemy.hit_points;
        damage(d, enemy, power, true, &mut self.next_status_id);
        let applied = (hp - enemy.hit_points) as usize;
        self.tally.strike_dmg += applied;
        self.tally.overkill += (power - enemy.armor.min(power)) as usize - applied;
        if enemy.alive() {
            apply_liquid(d, enemy, liquid, Source::Strike, &mut self.next_status_id);
        }
        enemy.aggroed = true;
        if liquid == Liquid::Pus && self.pus_infects {
            self.infect(target, Source::Strike);
        }
    }

    // ---- enemies ---------------------------------------------------------------------------

    /// `Creature.TakeTurn`: decide by personality, redirect, perform, after-act.
    fn enemy_turn(&mut self, i: usize) {
        // G17: the hunter knows the slime's cell every `scent_every` rounds.
        if let Some(m) = &self.map
            && m.hunter_at == Some(i)
            && m.hunter
                .as_ref()
                .is_some_and(|h| self.round.is_multiple_of(h.scent_every))
        {
            let memory = self.d.rules.pursuit_memory_turns;
            let cell = self.slime.cell;
            let e = &mut self.enemies[i];
            e.aggroed = true;
            e.pursuit = Some(cell);
            e.pursuit_left = memory.max(e.pursuit_left);
        }
        if self.enemies[i].hunter && self.can_see(i) {
            self.enemies[i].met = true;
        }
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
        if self.enemies[i].cell != from {
            self.step_in(i);
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
        // A miss can kill a suppressed attacker (R2): a dead enemy does nothing more.
        if !self.enemies[i].alive() {
            return;
        }
        self.after_act(i, intent);
        if self.enemies[i].hunter && self.can_see(i) {
            self.enemies[i].met = true;
        }
        self.alarm(i);
    }

    /// G4 `Alarm`: the first time an aggroed enemy sees the slime (after its own turn), it
    /// aggroes every living enemy within its radius (Chebyshev, through walls), except the
    /// hunter. Once per enemy; a woken enemy with an alarm raises its own when it sees the slime.
    fn alarm(&mut self, i: usize) {
        let radius = self.enemies[i].traits.iter().find_map(|t| match t {
            Trait::Alarm { radius } => Some(*radius),
            _ => None,
        });
        let Some(radius) = radius else { return };
        if self.enemies[i].alarmed || !self.enemies[i].aggroed || !self.can_see(i) {
            return;
        }
        self.enemies[i].alarmed = true;
        let (from, cell) = (self.enemies[i].cell, self.slime.cell);
        let memory = self.d.rules.pursuit_memory_turns;
        let hunter = self.map.as_ref().and_then(|m| m.hunter_at);
        for (j, e) in self.enemies.iter_mut().enumerate() {
            if j != i
                && Some(j) != hunter
                && e.alive()
                && !e.aggroed
                && chebyshev(e.cell, from) <= radius
            {
                e.aggroed = true;
                e.pursuit = Some(cell);
                e.pursuit_left = memory;
            }
        }
        self.note(|f| format!("{} raises the alarm", f.label(i)));
    }

    /// G17: the hunter enters at the exit (or the nearest free open cell) at round
    /// `enter_round`, or once the slime has entered the second-to-last room.
    fn enter_hunter(&mut self, round: u32) {
        let Some(m) = &self.map else { return };
        let Some(h) = m.hunter.as_ref().filter(|_| m.hunter_at.is_none()) else {
            return;
        };
        if round < h.enter_round && m.rooms_entered + 1 < m.rooms.len() {
            return;
        }
        let (kind, exit) = (h.kind.clone(), m.exit);
        let occupied = self.occupied();
        let mut cells: Vec<Cell> = (0..self.board.height)
            .flat_map(|y| (0..self.board.width).map(move |x| (x, y)))
            .filter(|&c| self.board.open(c) && !occupied.contains(&c))
            .collect();
        cells.sort_by_key(|&c| (chebyshev(c, exit), c.1, c.0));
        let Some(&cell) = cells.first() else { return };
        let mut hunter = enemy_creature(self.d, &kind, cell);
        hunter.aggroed = true;
        hunter.hunter = true;
        self.enemies.push(hunter);
        let m = self.map.as_mut().expect("checked");
        m.homes.push(None);
        m.hunter_at = Some(self.enemies.len() - 1);
        self.note(|f| format!("{} enters at {cell:?}", f.label(f.enemies.len() - 1)));
    }

    /// D5 `pus` on a map, with the bad affinity held. `false` hands the action to `kite`.
    /// - A Living enemy next to it: strike it from `pus_infects` levels on (or with the last
    ///   drop), else stand in the puddles. Other neighbours: `kite`.
    /// - In a fight: hold among the puddles; the Living come through them.
    /// - Calm and not full (and on Hard no hunter in yet): gather its Pus puddles (stand on one
    ///   and gather 1 unit per action, or step onto a neighbouring one), else wait for a burp.
    /// - Otherwise walk on like `kite`.
    fn pus_act(&mut self) -> bool {
        let hunted = self.map.as_ref().is_some_and(|m| m.hunter_at.is_some());
        let near = |f: &Fight, i: usize| {
            let e = &f.enemies[i];
            f.engaging(e) && chebyshev(e.cell, f.slime.cell) == 1 && !f.gives_up(i)
        };
        let adjacent: Vec<usize> = (0..self.enemies.len()).filter(|&i| near(self, i)).collect();
        // A non-Living neighbour (or the hunter, on Hard): `kite`, which never strikes the
        // Living with Pus before `pus_infects` levels (`kite_adjacent`).
        if hunted || adjacent.iter().any(|&i| !self.living(i)) {
            return false;
        }
        if let Some(&i) = adjacent.first() {
            if self.pus_infects || self.slime.volume.len() == 1 {
                self.strike_or_wait(i);
            } else if let Some(to) = self.kite_step() {
                // Step back so it has to come again, across the puddles the burps laid.
                let from = self.slime.cell;
                if self.move_slime(to) {
                    self.note(|_| format!("Slime steps back {from:?}->{to:?} behind its puddles"));
                }
            } else {
                self.note(|_| "Slime stands in its puddles".into());
            }
            return true;
        }
        let fight = (0..self.enemies.len()).any(|i| {
            self.living(i)
                && self.engaging(&self.enemies[i])
                && !self.gives_up(i)
                && self.can_reach(i)
        });
        if fight {
            self.note(|_| "Slime holds among its puddles".into());
            return true;
        }
        if self.slime.volume.len() >= self.d.slime.capacity || hunted {
            return false;
        }
        let here = self.slime.cell;
        if let Some(p) = self
            .puddles
            .iter_mut()
            .find(|p| p.0 == here && p.1 == Liquid::Pus)
        {
            p.2 -= 1;
            self.puddles.retain(|p| p.2 > 0);
            self.slime.volume.push(Liquid::Pus);
            let d = self.d;
            refresh_dominance(d, &mut self.slime, &mut self.next_status_id);
            self.note(|_| "Slime gathers its Pus".into());
            return true;
        }
        let occupied = self.occupied();
        let next = self
            .puddles
            .iter()
            .filter(|p| p.1 == Liquid::Pus && chebyshev(p.0, here) == 1)
            .map(|p| p.0)
            .filter(|c| !occupied.contains(c))
            .min_by_key(|c| (c.1, c.0));
        if let Some(to) = next {
            self.move_slime(to);
            return true;
        }
        let room = path::neighbours(here).any(|c| self.inside(c) && !occupied.contains(&c));
        if room {
            self.note(|_| "Slime waits to burp".into());
        }
        room
    }

    /// Enemy `i` is Living (`elements.living`).
    fn living(&self, i: usize) -> bool {
        let e = self.enemies[i].element;
        e.is_some() && e == self.d.elements.living
    }

    /// D3: the slime holds Pus as a bad affinity (a share of the body now, like every
    /// affinity).
    pub fn pus_affinity(&self) -> bool {
        let n = self.slime.volume.len();
        n > 0 && self.slime.count(Liquid::Pus) * 100 >= n * self.d.pus.affinity_percent
    }

    /// D3: this round is a burp round.
    fn burps(&self, round: u32) -> bool {
        self.pus_affinity() && round.is_multiple_of(self.d.pus.burp_every)
    }

    /// D3: `pus.burp_puddles` new 1-unit Pus puddles on free neighbouring cells (empty cells
    /// first, then 1-unit Pus puddles; row-major), from nothing.
    fn burp(&mut self) {
        let occupied = self.occupied();
        let mut cells: Vec<(usize, Cell)> = path::neighbours(self.slime.cell)
            .filter(|&c| self.inside(c) && !occupied.contains(&c))
            .filter_map(|c| match self.puddles.iter().find(|p| p.0 == c) {
                None => Some((0, c)),
                Some(&(_, Liquid::Pus, 1)) => Some((1, c)),
                Some(_) => None,
            })
            .collect();
        cells.sort_by_key(|&(k, c)| (k, c.1, c.0));
        for &(_, c) in cells.iter().take(self.d.pus.burp_puddles) {
            match self.puddles.iter_mut().find(|p| p.0 == c) {
                Some(p) => p.2 += 1,
                None => self.puddles.push((c, Liquid::Pus, 1)),
            }
            self.tally.burped += 1;
        }
        self.note(|f| format!("Slime burps; puddles {:?}", f.puddles));
    }

    /// D1: an enemy that steps into a puddle gets the liquid's effect without damage; a Pus
    /// puddle infects the Living.
    fn step_in(&mut self, i: usize) {
        let cell = self.enemies[i].cell;
        let Some(&(_, liquid, _)) = self.puddles.iter().find(|p| p.0 == cell) else {
            return;
        };
        if liquid == Liquid::Pus {
            return self.infect(i, Source::Puddle);
        }
        let d = self.d;
        apply_liquid(
            d,
            &mut self.enemies[i],
            liquid,
            Source::Puddle,
            &mut self.next_status_id,
        );
    }

    /// D3: Infected on a Living enemy; it refreshes, never stacks.
    fn infect(&mut self, i: usize, source: Source) {
        let d = self.d;
        let e = &mut self.enemies[i];
        if !e.alive() || e.element.is_none() || e.element != d.elements.living {
            return;
        }
        let pulses = d.pus.infected_pulses;
        if let Some(s) = e
            .statuses
            .iter_mut()
            .find(|s| s.kind == StatusKind::Infected)
        {
            s.remaining = s.remaining.max(pulses);
        } else {
            add_status(
                e,
                StatusKind::Infected,
                pulses,
                source,
                &mut self.next_status_id,
            );
        }
    }

    /// R1: at the end of the slime's actions, the reacting pair nearest the edge becomes its
    /// product, the body closes up, and so on until no neighbours react. A chain with a stone
    /// reaction drops one stone next to the slime. Old stones crumble first.
    fn react(&mut self) {
        for s in &mut self.stones {
            s.1 -= 1;
        }
        self.stones.retain(|s| s.1 > 0);
        let d = self.d;
        // One blocker per stone pair that reacted in the chain (a stone, an amber).
        let mut stones: Vec<(Liquid, Liquid)> = Vec::new();
        for _ in 0..1000 {
            let v = &self.slime.volume;
            let pair = (0..v.len().saturating_sub(1)).rev().find_map(|k| {
                d.reactions
                    .iter()
                    .find(|r| {
                        (r.pair == (v[k], v[k + 1]) || r.pair == (v[k + 1], v[k]))
                            && (d.rules.reaction_can_kill || v.len() > 2 || !r.makes.is_empty())
                    })
                    .map(|r| (k, r))
            });
            let Some((k, r)) = pair else {
                for _ in 0..stones.len() {
                    self.drop_stone();
                }
                let d = self.d;
                refresh_dominance(d, &mut self.slime, &mut self.next_status_id);
                return;
            };
            let (a, b) = (self.slime.volume[k], self.slime.volume[k + 1]);
            self.slime.volume.splice(k..k + 2, r.makes.iter().copied());
            self.tally.reactions += 1;
            self.tally.react_loss += 2 - r.makes.len();
            if r.stone && !stones.contains(&r.pair) {
                stones.push(r.pair);
            }
            self.note(|_| format!("Slime's {a:?} and {b:?} react: {:?}", r.makes));
            if !self.slime.alive() {
                self.last_harm = Some("reaction".into());
                return;
            }
        }
        panic!("a reaction chain did not end (data/design.toml [[reactions]])");
    }

    /// A stone on the free open neighbour of the slime nearest the nearest living enemy.
    fn drop_stone(&mut self) {
        let occupied = self.occupied();
        let enemy = self
            .enemies
            .iter()
            .filter(|e| e.alive())
            .min_by_key(|e| chebyshev(e.cell, self.slime.cell))
            .map(|e| e.cell);
        let cell = path::neighbours(self.slime.cell)
            // Never on the exit or a drip: a stone must not close the way out or a source.
            .filter(|&c| self.inside(c) && !occupied.contains(&c))
            .filter(|&c| {
                self.map
                    .as_ref()
                    .is_none_or(|m| c != m.exit && m.drips.iter().all(|d| d.0 != c))
            })
            .min_by_key(|&c| (enemy.map_or(0, |e| chebyshev(c, e)), c.1, c.0));
        if let Some(cell) = cell {
            self.stones.push((cell, self.d.rules.stone_rounds));
            self.tally.stones += 1;
            self.note(|_| format!("A stone falls at {cell:?}"));
        }
    }

    /// G5: at the end of the slime's actions, while an enemy is engaged, each Pus unit turns
    /// its Blood neighbours (in body order) into Pus; anything else between them stops it.
    fn fester(&mut self) {
        // Like Regeneration (G3), it ticks only in a fight: outside one there is no clock.
        if !self.enemies.iter().any(|e| self.engaging(e)) {
            return;
        }
        let v = &mut self.slime.volume;
        let pus: Vec<usize> = (0..v.len()).filter(|&k| v[k] == Liquid::Pus).collect();
        for k in pus {
            for n in [k.wrapping_sub(1), k + 1] {
                if v.get(n) == Some(&Liquid::Blood) {
                    v[n] = Liquid::Pus;
                }
            }
        }
        let d = self.d;
        refresh_dominance(d, &mut self.slime, &mut self.next_status_id);
    }

    /// `IsDetectedBy`: an aggroed observer always detects; Mercury dominance hides the slime.
    fn detected(&self, i: usize) -> bool {
        self.slime.alive() && (self.enemies[i].aggroed || !self.invisible())
    }

    fn can_see(&self, i: usize) -> bool {
        chebyshev(self.enemies[i].cell, self.slime.cell) <= self.sight(&self.enemies[i])
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
        // No bite (melee_power 0): not an attack at all.
        if !self.detected(i) || self.enemies[i].melee_power <= 0 {
            return;
        }
        let d = self.d;
        // R2: the enemy's element against the slime's edge unit's.
        let el = &d.elements;
        let (theirs, mine) = (
            self.enemies[i].element,
            self.slime.volume.last().and_then(|&l| el.of(l)),
        );
        if theirs.is_some() && theirs == mine {
            return self.note(|f| format!("{} cannot hurt its own element", f.label(i)));
        }
        let mut power = self.enemies[i].melee_power;
        if self.enemies[i]
            .statuses
            .iter()
            .any(|s| s.kind == StatusKind::Infected)
        {
            power = (power - d.pus.infected_bite).max(0);
            if power == 0 {
                return self.note(|f| format!("{} is too sick to bite", f.label(i)));
            }
        }
        if theirs.zip(mine).is_some_and(|(a, b)| el.suppresses(b, a))
            && el.suppressed_hit_percent < 100
            && self.rng.pick(100) >= el.suppressed_hit_percent
        {
            // A suppressed attacker misses and hurts itself.
            let e = &mut self.enemies[i];
            e.hit_points = (e.hit_points - el.miss_self_damage).max(0);
            e.aggroed = true;
            self.tally.elem_miss += 1;
            return self.note(|f| format!("{} misses and hurts itself", f.label(i)));
        }
        if theirs.zip(mine).is_some_and(|(a, b)| el.suppresses(a, b)) {
            power = if el.suppress == "double" {
                power * 2
            } else {
                power + 1
            };
        }
        // Retaliation is captured before the hit: the hit may end dominance.
        let retort = self.slime.dominant;
        let before = self.slime.volume.clone();
        let tip = if retort == Some(Liquid::Mercury) && d.rules.mercury_affinity == "defence" {
            // D7: half the bite (rounded up), the edge-most non-Mercury units first.
            let v = &mut self.slime.volume;
            let mut tip = None;
            for _ in 0..(power + 1) / 2 {
                let Some(k) = v
                    .iter()
                    .rposition(|&l| l != Liquid::Mercury)
                    .or(v.len().checked_sub(1))
                else {
                    break;
                };
                tip.get_or_insert(v.remove(k));
            }
            if self.slime.alive() {
                refresh_dominance(d, &mut self.slime, &mut self.next_status_id);
            }
            tip
        } else {
            damage(d, &mut self.slime, power, true, &mut self.next_status_id)
        };
        self.units_bitten += before.len() - self.slime.volume.len();
        let mut left = self.slime.volume.clone();
        for unit in before {
            match left.iter().position(|&l| l == unit) {
                Some(k) => {
                    left.remove(k);
                }
                None => *self.tally.bitten_by.entry(unit).or_insert(0) += 1,
            }
        }
        if tip.is_some() {
            self.last_harm = Some(format!("melee:{}", self.enemies[i].name));
        }
        if !self.slime.alive()
            && let Some(m) = self.map.as_mut()
            && m.hunter_at == Some(i)
        {
            m.hunter_killed_slime = true;
        }
        let traits = &self.enemies[i].traits;
        if self.slime.alive()
            && traits.contains(&Trait::Infects)
            && let Some(k) = self.slime.volume.iter().rposition(|&l| l == Liquid::Blood)
        {
            self.slime.volume[k] = Liquid::Pus;
            refresh_dominance(d, &mut self.slime, &mut self.next_status_id);
        }
        if self.slime.alive() && traits.contains(&Trait::Nets) {
            self.rooted = 1;
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
        let speed = e.speed(self.d);
        if let Some(to) = self.walk_toward(e.cell, target, speed) {
            self.enemies[i].cell = to;
            // G1 dash: a fast walker that arrives next to the slime bites at once.
            if speed >= 2
                && self.enemies[i].traits.contains(&Trait::Dash)
                && self.adjacent_and_detected(i)
            {
                self.enemy_melee(i);
            }
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
                // A creature without a bite (melee_power 0) does not attack.
                if (intent == Intent::Attack && e.melee_power > 0) || sees {
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
        let regenerates =
            !d.rules.regeneration_in_fight_only || self.enemies.iter().any(|e| self.engaging(e));
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
                StatusKind::Regeneration if !regenerates => {}
                StatusKind::Regeneration => {
                    for _ in 0..d.statuses.regeneration_growth {
                        if owner.volume.len() < d.slime.capacity {
                            owner.volume.push(Liquid::Blood);
                        }
                    }
                }
                StatusKind::Infected => {
                    harm = d.pus.infected_damage;
                    damage(d, owner, harm, false, ids);
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
                    Source::Strike | Source::Body | Source::Puddle => t.status_dmg += applied,
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
            // The dead keep the status that killed them (a Poisoned corpse, D8).
            if !owner.alive() {
                break;
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

/// `Creature.ModifyIncomingHarm`: ×multiplier per Flammable in the queue. (The game's Oil
/// dominance, Forever Flammable, went with Oil: Oleg removed Oil and Tar.)
fn modify_incoming_harm(d: &Design, c: &Creature, mut amount: i32) -> i32 {
    if amount <= 0 {
        return amount;
    }
    for s in &c.statuses {
        if s.kind == StatusKind::Flammable {
            amount *= d.statuses.flammable_multiplier;
        }
    }
    amount
}

/// `Substance.Apply`: what a liquid leaves on whoever it touches (strike, retaliation, puddle),
/// then `NotifyReceivedSubstance` (Vampirism drinks Blood).
fn apply_liquid(d: &Design, target: &mut Creature, liquid: Liquid, from: Source, ids: &mut u32) {
    let s = &d.statuses;
    // `rules.retaliation_pulses` (design): Retaliation statuses are short.
    let pulses = d.rules.retaliation_pulses;
    let own = d.liquid(liquid).pulses;
    let element = target.element;
    let own_immune = target.status_immune.clone();
    let has = |table: &BTreeMap<String, Vec<Liquid>>, kind: StatusKind| {
        element.is_some_and(|e| table.get(kind.name()).is_some_and(|l| l.contains(&e)))
    };
    let mut add = |kind: StatusKind, duration| {
        // R2 immunity by the target's element or its own: the strike hurt, the status does not.
        if has(&d.elements.status_immune, kind) || own_immune.iter().any(|n| n == kind.name()) {
            return;
        }
        let duration = if from == Source::Retaliation && pulses > 0 {
            pulses
        } else {
            own.unwrap_or(duration)
        };
        let copies = if has(&d.elements.status_double, kind) {
            2
        } else {
            1
        };
        for _ in 0..copies {
            add_status(target, kind, duration, from, ids);
        }
    };
    match liquid {
        Liquid::Water => add(StatusKind::Wet, s.wet),
        Liquid::Poison => add(StatusKind::Poisoned, s.poisoned),
        Liquid::Acid => {
            let corrosion = d.liquid(Liquid::Acid).corrosion;
            add(StatusKind::Corroding { corrosion }, s.corroding);
        }
        Liquid::Lava => add(StatusKind::Burning, s.burning),
        Liquid::Vitriol => {
            // Its Corroding strips at least 1 armor per pulse; the strike itself strips none.
            let corrosion = d.liquid(Liquid::Vitriol).corrosion.max(1);
            add(StatusKind::Corroding { corrosion }, s.corroding);
        }
        Liquid::Alkali => add(StatusKind::Instability, s.instability),
        Liquid::Sulfur => add(StatusKind::Flammable, s.flammable),
        // No status in the model yet (Cold fire, Greased, Rooted, Charmed, Infected), or none.
        Liquid::Blood
        | Liquid::Mercury
        | Liquid::Pus
        | Liquid::Phosphorus
        | Liquid::Aquavita
        | Liquid::Ambrosia
        | Liquid::Fat
        | Liquid::Resin
        | Liquid::Aphrodisiac
        | Liquid::Dross => {}
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
        .filter(|&(_, n)| {
            let basis = if d.rules.dominance_basis == "capacity" {
                d.slime.capacity
            } else {
                c.volume.len()
            };
            n * 100 >= basis * d.rules.dominance_percent
        })
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

/// An enemy of `kind` standing at `cell`, from its `[[enemies]]` row.
fn enemy_creature(d: &Design, kind: &str, cell: Cell) -> Creature {
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
        element: d.elements.of_enemy(e),
        status_immune: e.status_immune.clone(),
        traits: e.traits.clone(),
        vision: e.vision,
        hunter: false,
        met: false,
        alarmed: false,
        thorns: e.thorns,
        dominant: None,
        aggroed: false,
        pursuit: None,
        pursuit_left: 0,
        flee_cell: None,
    }
}
