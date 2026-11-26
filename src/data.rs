//! The design numbers, loaded from `data/design.toml`. Nothing tunable lives in code.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Liquid {
    Water,
    Poison,
    Acid,
    Blood,
    Lava,
    Mercury,
    /// An infected Blood unit (`Infects`): no power, no effect, spat off the edge.
    Pus,
    // The rest of the 16 liquids (17 with Ectoplasm, later; lore `forum.md`, balance
    // `liquid-roles.md`): a strike without a modelled status hits for its `power` alone.
    Ichor,
    Aquavita,
    Vitriol,
    Alkali,
    Ambrosia,
    Fat,
    Resin,
    Sulfur,
    Aphrodisiac,
    /// Dead cargo a stop leaves behind (spent Ichor, Fat): no power, no effect, a cell.
    Dross,
}

pub const ALL_LIQUIDS: [Liquid; 17] = [
    Liquid::Water,
    Liquid::Poison,
    Liquid::Acid,
    Liquid::Blood,
    Liquid::Lava,
    Liquid::Mercury,
    Liquid::Pus,
    Liquid::Ichor,
    Liquid::Aquavita,
    Liquid::Vitriol,
    Liquid::Alkali,
    Liquid::Ambrosia,
    Liquid::Fat,
    Liquid::Resin,
    Liquid::Sulfur,
    Liquid::Aphrodisiac,
    Liquid::Dross,
];

impl Liquid {
    /// `lava` → Lava: the liquid in a `b-<liquid>` policy name.
    pub fn from_lower(name: &str) -> Option<Liquid> {
        ALL_LIQUIDS
            .into_iter()
            .find(|l| format!("{l:?}").to_lowercase() == name)
    }
}

/// D8: `b-auto` or `b-<liquid>` (any liquid but Dross).
pub fn build_policy(name: &str) -> bool {
    name == "b-auto"
        || name
            .strip_prefix("b-")
            .and_then(Liquid::from_lower)
            .is_some_and(|l| l != Liquid::Dross)
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Personality {
    /// Chases and attacks the slime it sees.
    Aggressive,
    /// Ignores the slime until hit, then behaves as Aggressive (sticky aggro).
    Passive,
    /// Flees the slime it sees; attacks once only when aggroed and adjacent.
    Cowardly,
}

/// Enemy traits. Only `Vampirism` is in the game; the rest are agreed design rules for the
/// 20-level graph (balance `sim-graph20-spec.md`, `squad.md`).
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trait {
    /// Heals `statuses.vampirism_heal` HP when it receives Blood or strikes a Blood unit off the slime.
    Vampirism,
    /// G1: after a chase move that ends next to the slime, it bites in the same turn.
    Dash,
    /// G2: sees the slime 2 cells farther while the slime's edge unit is Blood.
    SmellsBlood,
    /// G4: on first sight of the slime, aggroes every living enemy within `radius`
    /// (Chebyshev, through walls).
    Alarm { radius: i32 },
    /// G5: its bite turns the slime's edge-most Blood into Pus.
    Infects,
    /// Squad net: its bite also roots the slime for its next turn (no steps, strikes allowed).
    Nets,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Design {
    pub rules: Rules,
    pub slime: SlimeData,
    pub statuses: StatusData,
    pub liquids: Vec<LiquidData>,
    pub enemies: Vec<EnemyData>,
    pub matrix: Matrix,
    pub policies: Policies,
    /// Level runs for the body economy (`src/level.rs`); none is fine.
    #[serde(default)]
    pub levels: Vec<Level>,
    /// Pass rules for the ladder summary (`src/ladder.rs`); none is fine.
    #[serde(default)]
    pub ladder: LadderRules,
    /// The two element circles (R1 encouragement, R2 suppression); none is fine.
    #[serde(default)]
    pub elements: Elements,
    /// R1: pairs of neighbouring units that react in the body; none is fine.
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    /// D3: Pus as a bad affinity, its burps and Infected (balance `sim-3d-spec.md`).
    pub pus: PusRules,
}

/// `[pus]` (D3).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PusRules {
    /// Bad affinity: Pus at least this percent of the body now.
    pub affinity_percent: usize,
    /// While it holds, every this many slime rounds the slime burps: it skips its turn and
    /// `burp_puddles` new 1-unit Pus puddles fall on free neighbouring cells.
    pub burp_every: u32,
    pub burp_puddles: usize,
    /// Infected (Living enemies only, from Pus puddles, and from Pus hits on `pus_infects`
    /// levels): pulses, bite reduction, damage per pulse. It refreshes, never stacks.
    pub infected_pulses: i32,
    pub infected_bite: i32,
    pub infected_damage: i32,
}

/// `[elements]` (balance `sim-reactions-spec.md`, lore `forum.md`).
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Elements {
    /// Each circle in order: a unit encourages the next one (A → B).
    #[serde(default)]
    pub circles: Vec<Vec<Liquid>>,
    /// Extra strike damage, once per slime turn, when the unit under the edge encourages the
    /// edge unit.
    #[serde(default)]
    pub encouragement_bonus: i32,
    /// R2: a dominating hit is `plus1` (+1) or `double` (x2).
    #[serde(default = "plus1")]
    pub suppress: String,
    /// R2: a suppressed enemy's bite lands this percent of the time...
    #[serde(default = "hundred")]
    pub suppressed_hit_percent: usize,
    /// ...and a miss costs it this many HP.
    #[serde(default)]
    pub miss_self_damage: i32,
    /// The element of Living creatures: Ambrosia heals them instead of hurting.
    pub living: Option<Liquid>,
    /// An enemy without `element` takes its corpse's main liquid's element: a circle liquid
    /// is its own, the others map here (lore forum.md §3).
    #[serde(default)]
    pub corpse: BTreeMap<Liquid, Liquid>,
    /// Statuses (by name) that do not take on enemies of these elements.
    #[serde(default)]
    pub status_immune: BTreeMap<String, Vec<Liquid>>,
    /// Statuses that take twice (two copies) on enemies of these elements.
    #[serde(default)]
    pub status_double: BTreeMap<String, Vec<Liquid>>,
}

fn plus1() -> String {
    "plus1".into()
}

fn hundred() -> usize {
    100
}

impl Elements {
    /// `a` encourages `b`: `b` follows `a` in a circle.
    pub fn encourages(&self, a: Liquid, b: Liquid) -> bool {
        self.circles.iter().any(|c| {
            c.iter()
                .position(|&l| l == a)
                .is_some_and(|i| c[(i + 1) % c.len()] == b)
        })
    }

    /// `a` ⊣ `b`: `b` is two places after `a` in a circle.
    pub fn suppresses(&self, a: Liquid, b: Liquid) -> bool {
        self.circles.iter().any(|c| {
            c.iter()
                .position(|&l| l == a)
                .is_some_and(|i| c[(i + 2) % c.len()] == b)
        })
    }

    /// A liquid's element: circle liquids are their own; the others have none.
    pub fn of(&self, l: Liquid) -> Option<Liquid> {
        self.circles.iter().any(|c| c.contains(&l)).then_some(l)
    }

    /// An enemy's element: `element`, else its corpse's most common liquid (the first on a
    /// tie) as its own element or through `corpse`.
    pub fn of_enemy(&self, e: &EnemyData) -> Option<Liquid> {
        if e.element.is_some() {
            return e.element;
        }
        let main = e
            .volume
            .iter()
            .max_by_key(|&&l| {
                let first = e.volume.iter().position(|&x| x == l);
                (
                    e.volume.iter().filter(|&&x| x == l).count(),
                    std::cmp::Reverse(first),
                )
            })
            .copied()?;
        self.of(main).or_else(|| self.corpse.get(&main).copied())
    }
}

/// `[[reactions]]`: two neighbouring units (either order) become `makes`.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Reaction {
    pub pair: (Liquid, Liquid),
    pub makes: Vec<Liquid>,
    /// It drops a stone next to the slime (Water + Lava).
    #[serde(default)]
    pub stone: bool,
}

/// `[ladder]`: what counts as passing a ladder level, per difficulty and policy.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct LadderRules {
    #[serde(default)]
    pub thresholds: Vec<Threshold>,
    /// D4: the stops between maps (run mode): resets of the body, then a Water refill.
    #[serde(default)]
    pub stops: Vec<Stop>,
}

/// A stop after ladder level `after`: the carried body is reset by `resets` in order, then its
/// empty cells (up to the next level's capacity) fill with Water.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Stop {
    pub after: u32,
    #[serde(default)]
    pub resets: Vec<Reset>,
}

/// Every `from` unit becomes `to` (or leaves the body without `to`); with `max_percent`, at
/// most that percent of the body's units change (rounded down), edge-most first.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Reset {
    pub from: Liquid,
    pub to: Option<Liquid>,
    pub max_percent: Option<usize>,
}

/// A level passes when its `end` is at least `min` (or at most `max`) percent.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Threshold {
    pub difficulty: String,
    pub policy: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl Threshold {
    /// `min90` or `max10`.
    pub fn rule(&self) -> String {
        match (self.min, self.max) {
            (Some(v), _) => format!("min{v}"),
            (_, Some(v)) => format!("max{v}"),
            _ => unreachable!("checked in parse"),
        }
    }

    pub fn meets(&self, end: f64) -> bool {
        self.min.is_none_or(|v| end >= v) && self.max.is_none_or(|v| end <= v)
    }
}

/// A map level's place on the 20-level ladder.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LadderPlace {
    pub level: u32,
    /// `E`, `N` or `H`.
    pub difficulty: String,
    /// The city choices made so far, joined with `-`; empty before the first city.
    #[serde(default)]
    pub branch: String,
}

impl LadderPlace {
    /// `parent` is a branch this one continues: the same, or this one with choices dropped.
    pub fn continues(&self, parent: &str) -> bool {
        parent.is_empty() || self.branch == parent || self.branch.starts_with(&format!("{parent}-"))
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Rules {
    pub board_width: i32,
    pub board_height: i32,
    pub vision_range: i32,
    pub dominance_percent: usize,
    /// What `dominance_percent` is a share of: `current` (the body now; the design and the game)
    /// or `capacity` (fallback).
    pub dominance_basis: String,
    /// Retaliation statuses last this many pulses (the game: the liquid's usual duration, 0).
    pub retaliation_pulses: i32,
    /// 4c (design): without an affinity, the edge unit a bite takes leaves its status on the
    /// biter for this many pulses (2: one of its turns); 0 turns it off (the fallback).
    pub edge_retort_pulses: i32,
    /// Heavy liquids: holding at least this many units costs 1 action point each (the design).
    pub heavy: BTreeMap<Liquid, usize>,
    /// Heaviness never lowers the action points below this, or below the base when that is lower.
    pub heavy_min_action_points: u32,
    /// R1: a reaction chain may empty the body (the slime dies); `false`: a pair that would
    /// does not react.
    pub reaction_can_kill: bool,
    /// Slime rounds a reaction stone stands before it crumbles.
    pub stone_rounds: u32,
    pub pursuit_memory_turns: i32,
    pub max_rounds: u32,
    /// П15, an agreed design rule: at 1 unit a strike hits and spends nothing. The game does
    /// not implement it yet (its last unit cannot be spent); `false` models the game today.
    pub last_drop_strike: bool,
    /// Spit range (Chebyshev). Only 1 is modelled: spit replaces a strike on a neighbour.
    pub spit_range: i32,
    /// G3, an agreed design rule: Regeneration grows Blood only while an enemy is engaged with
    /// the slime. Not in the game yet.
    pub regeneration_in_fight_only: bool,
    /// D7, design: what a Mercury affinity does. `defence`: a bite deals half (rounded up) and
    /// takes the edge-most non-Mercury unit while one is left. `invisible` (the game, fallback):
    /// an enemy that is not aggroed cannot see, chase or bite the slime.
    pub mercury_affinity: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SlimeData {
    pub capacity: usize,
    pub speed: i32,
    pub start: Vec<Liquid>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct StatusData {
    pub wet: i32,
    pub burning: i32,
    pub poisoned: i32,
    pub corroding: i32,
    pub instability: i32,
    pub flammable: i32,
    pub burning_pulse: i32,
    pub poisoned_pulse: i32,
    pub corroding_pulse: i32,
    pub flammable_multiplier: i32,
    /// How Flammable statuses stack: `additive` (the design) or `multiplicative` (the game).
    pub flammable_stacking: String,
    pub instability_speed: i32,
    pub vampirism_heal: i32,
    pub regeneration_growth: usize,
    /// 4a, Ichor's strike: Cold fire for `cold_fire` pulses of `cold_fire_pulse` HP; Blinded
    /// for `blinded` pulses, sight −`blinded_vision` (at least 1).
    pub cold_fire: i32,
    pub cold_fire_pulse: i32,
    pub blinded: i32,
    pub blinded_vision: i32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LiquidData {
    pub name: Liquid,
    pub power: i32,
    pub corrosion: i32,
    /// Pulses of this liquid's strike statuses, when they differ from `[statuses]`.
    pub pulses: Option<i32>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct EnemyData {
    pub name: String,
    pub hit_points: i32,
    pub armor: i32,
    pub speed: i32,
    pub melee_power: i32,
    pub volume: Vec<Liquid>,
    pub personality: Personality,
    pub traits: Vec<Trait>,
    /// Thorns: a melee strike on this enemy costs the slime this many edge units (level 2).
    #[serde(default)]
    pub thorns: usize,
    /// Its own sight range; the level's (or `rules.vision_range`) when absent.
    pub vision: Option<i32>,
    /// R2: its element; derived from the corpse when absent (`Elements::of_enemy`).
    pub element: Option<Liquid>,
    /// Statuses (by name) that never take on it, on top of `elements.status_immune`.
    #[serde(default)]
    pub status_immune: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Matrix {
    pub seeds: u64,
    pub action_points: Vec<u32>,
    pub pure_build_units: usize,
    pub enemy_counts: Vec<usize>,
    pub slime_cell: (i32, i32),
    pub enemy_column_offset: i32,
    pub enemy_rows: Vec<i32>,
    /// The liquids with a pure build in the matrix.
    pub build_liquids: Vec<Liquid>,
    /// Slime policies the matrix runs (`newcomer`, `arranger`).
    pub policies: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Policies {
    /// Actions the `lure` policy waits on its way into a room while 2+ calm enemies would see
    /// it there.
    pub lure_patience: u32,
    /// The liquid `arranger` keeps deep, or `none`.
    pub arranger_keep: String,
}

impl Policies {
    /// `arranger_keep` as a liquid; `None` for "none" (validated in `Design::parse`).
    pub fn keep(&self) -> Option<Liquid> {
        toml::Value::String(self.arranger_keep.clone())
            .try_into()
            .ok()
    }
}

/// A level: stages played in order, the slime's body carried from one to the next.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Level {
    pub id: String,
    pub capacity: usize,
    /// The body at the level start; full Water by default.
    pub start: Option<Vec<Liquid>>,
    pub action_points: u32,
    /// `newcomer` for a stage level, `strafe` or `lure` for a map level.
    pub policy: String,
    /// A stage level: fights and fountains in order (`src/level.rs`).
    #[serde(default)]
    pub stages: Vec<Stage>,
    /// A map level: rooms the slime walks through to the exit (`src/map.rs`).
    pub map: Option<MapSpec>,
    /// A map level on the ladder (`src/ladder.rs`); it is reported in `[ladder]` only.
    pub ladder: Option<LadderPlace>,
    /// D3: Pus hits apply Infected here (from level 9, after the undertaker).
    #[serde(default)]
    pub pus_infects: bool,
}

/// A generated map: a west-to-east chain of rooms joined by corridors.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct MapSpec {
    pub rooms: usize,
    /// Room width and height, each random in `[min, max]`, inner cells.
    pub room_side: (i32, i32),
    pub enemies_per_room: (usize, usize),
    /// Each enemy's kind is a random pick from this list.
    pub enemy_kinds: Vec<String>,
    /// Corridor cells between the two doorways of neighbouring rooms.
    pub corridor: i32,
    /// Enemy sight range (Chebyshev), blocked by walls.
    pub vision: i32,
    /// Kinds placed in doorways and corridors instead of rooms (a random pick each).
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub blockers_per_level: usize,
    /// Units the slime can drink at the exit before leaving, 1 per action, until full.
    #[serde(default)]
    pub river: usize,
    /// The river's liquids, drunk in this order, cycling.
    #[serde(default)]
    pub river_liquids: Vec<Liquid>,
    /// Water drips on corridor and doorway cells (which the slime always crosses).
    pub drips: Option<Drips>,
    /// The middle rooms of the chain lie under the open sky (sun).
    #[serde(default)]
    pub surface_rooms: usize,
    /// In a surface room the sun takes the edge-most Water after every this many slime rounds;
    /// 0 without sun.
    #[serde(default)]
    pub sun_every: u32,
    /// G17: an enemy that enters at the exit and comes for the slime.
    pub hunter: Option<Hunter>,
    /// Kinds camped in the second-to-last room (the squad), one enemy each.
    #[serde(default)]
    pub camp: Vec<String>,
    /// Roaming singles from a spawner (Oleg; game `docs/design/level-generator.md`).
    pub spawner: Option<Spawner>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Spawner {
    /// It releases one enemy of a random kind from this list...
    pub kinds: Vec<String>,
    /// ...after every this many slime rounds...
    pub every: u32,
    /// ...while fewer than this many of its enemies are alive.
    pub max_alive: usize,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Hunter {
    pub kind: String,
    /// It enters at the start of this round's enemy phase, or earlier when the slime enters
    /// the second-to-last room.
    pub enter_round: u32,
    /// It learns the slime's cell every this many rounds (1: always knows it).
    #[serde(default = "one")]
    pub scent_every: u32,
}

fn one() -> u32 {
    1
}

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct Drips {
    pub cells: usize,
    /// Water units on each drip cell.
    pub units: usize,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Stage {
    /// `enemies` of `enemy` at once; the slime eats every corpse after a won fight.
    Fight { enemy: String, enemies: usize },
    /// The slime drinks up to `units` of `liquid` until full; the rest is lost.
    Fountain { liquid: Liquid, units: usize },
}

impl Design {
    pub fn load(path: &Path) -> Result<Design, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Design::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Design, String> {
        let design: Design = toml::from_str(text).map_err(|e| e.to_string())?;
        for liquid in ALL_LIQUIDS {
            let count = design.liquids.iter().filter(|l| l.name == liquid).count();
            if count != 1 {
                return Err(format!(
                    "[[liquids]] must list {liquid:?} exactly once, found {count}"
                ));
            }
        }
        let max_count = design
            .matrix
            .enemy_counts
            .iter()
            .max()
            .copied()
            .unwrap_or(0);
        if design.matrix.enemy_rows.len() < max_count {
            return Err(format!(
                "matrix.enemy_rows has {} rows, encounters need {max_count}",
                design.matrix.enemy_rows.len()
            ));
        }
        if !(1..=100).contains(&design.rules.dominance_percent) {
            return Err("rules.dominance_percent must be 1..=100".into());
        }
        if design.statuses.flammable_multiplier < 1 {
            return Err("statuses.flammable_multiplier must be >= 1".into());
        }
        if !["additive", "multiplicative"].contains(&design.statuses.flammable_stacking.as_str()) {
            return Err(
                "statuses.flammable_stacking must be \"additive\" or \"multiplicative\"".into(),
            );
        }
        if !["defence", "invisible"].contains(&design.rules.mercury_affinity.as_str()) {
            return Err("rules.mercury_affinity must be \"defence\" or \"invisible\"".into());
        }
        if !["capacity", "current"].contains(&design.rules.dominance_basis.as_str()) {
            return Err("rules.dominance_basis must be \"current\" or \"capacity\"".into());
        }
        if design.rules.edge_retort_pulses < 0 {
            return Err("rules.edge_retort_pulses must be >= 0 (0: off)".into());
        }
        if design.rules.retaliation_pulses < 0 {
            return Err(
                "rules.retaliation_pulses must be >= 0 (0: the liquid's own duration)".into(),
            );
        }
        if design.rules.heavy.values().any(|&n| n == 0) {
            return Err("rules.heavy: each count must be at least 1".into());
        }
        if design.rules.heavy_min_action_points == 0 {
            return Err("rules.heavy_min_action_points must be at least 1".into());
        }
        let b = &design.matrix.build_liquids;
        if b.is_empty() || (1..b.len()).any(|i| b[..i].contains(&b[i])) {
            return Err("matrix.build_liquids: list at least one liquid, each once".into());
        }
        if let Some(r) = design.reactions.iter().find(|r| {
            r.makes.len() > 1
                && design
                    .reactions
                    .iter()
                    .any(|q| r.makes.contains(&q.pair.0) || r.makes.contains(&q.pair.1))
        }) {
            return Err(format!(
                "reactions {:?}: a product of two or more units must not react again (the chain must end)",
                r.pair
            ));
        }
        if design.reactions.iter().any(|r| r.makes.len() > 2) || design.rules.stone_rounds == 0 {
            return Err(
                "reactions make at most 2 units, and rules.stone_rounds must be >= 1".into(),
            );
        }
        let el = &design.elements;
        let known = [
            "Wet",
            "Burning",
            "Poisoned",
            "Corroding",
            "Instability",
            "Flammable",
            "Regeneration",
            "Infected",
            "ColdFire",
            "Blinded",
        ];
        if !["plus1", "double"].contains(&el.suppress.as_str())
            || el.suppressed_hit_percent > 100
            || el.miss_self_damage < 0
        {
            return Err(
                "elements: suppress is plus1 or double, suppressed_hit_percent 0..=100, miss_self_damage >= 0"
                    .into(),
            );
        }
        if let Some(name) = el
            .status_immune
            .keys()
            .chain(el.status_double.keys())
            .chain(design.enemies.iter().flat_map(|e| &e.status_immune))
            .find(|k| !known.contains(&k.as_str()))
        {
            return Err(format!(
                "elements: no status named {name:?} in the model (known: {known:?})"
            ));
        }
        if design.elements.circles.iter().any(|c| c.len() < 3) {
            return Err("elements.circles: each circle needs at least 3 liquids".into());
        }
        let pus = &design.pus;
        if pus.burp_every == 0 || pus.infected_pulses < 1 {
            return Err("pus.burp_every and pus.infected_pulses must be at least 1".into());
        }
        if design.rules.spit_range != 1 {
            return Err("rules.spit_range: only 1 is modelled".into());
        }
        if design.policies.arranger_keep != "none" && design.policies.keep().is_none() {
            return Err(format!(
                "policies.arranger_keep must be \"none\" or a liquid, not {:?}",
                design.policies.arranger_keep
            ));
        }
        let policies = &design.matrix.policies;
        if let Some(p) = policies.iter().find(|p| {
            !["newcomer", "arranger", "kite", "reader", "pus", "mercury"].contains(&p.as_str())
                && !build_policy(p)
        }) {
            return Err(format!("matrix.policies: unknown policy {p:?}"));
        }
        if (1..policies.len()).any(|i| policies[..i].contains(&policies[i])) {
            return Err("matrix.policies: a policy is listed twice".into());
        }
        for e in &design.enemies {
            if e.hit_points < 1 || e.armor < 0 || e.speed < 0 || e.melee_power < 0 {
                return Err(format!(
                    "enemy {}: hit_points must be >= 1; armor, speed, melee_power >= 0",
                    e.name
                ));
            }
            if e.vision.is_some_and(|v| v < 1)
                || e.traits
                    .iter()
                    .any(|t| matches!(t, Trait::Alarm { radius } if *radius < 1))
            {
                return Err(format!(
                    "enemy {}: vision and Alarm radius must be >= 1",
                    e.name
                ));
            }
        }
        for (i, level) in design.levels.iter().enumerate() {
            let problem = if design.levels[..i].iter().any(|l| l.id == level.id) {
                Some("duplicate id")
            } else if level.id.is_empty() || level.id.len() > 14 {
                Some("id must be 1..=14 characters (it is a fixed-width column)")
            } else if level.capacity == 0 || level.action_points == 0 {
                Some("capacity and action_points must be at least 1")
            } else if level.stages.is_empty() == level.map.is_none() {
                Some("give either stages or map, not both")
            } else if level.stages.len() >= 1000 {
                Some("stages must have fewer than 1000 entries")
            } else if level
                .start
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > level.capacity)
            {
                Some("start must have 1..=capacity units")
            } else {
                None
            };
            if let Some(problem) = problem {
                return Err(format!("level {:?}: {problem}", level.id));
            }
            let allowed: &[&str] = if level.map.is_some() {
                &["strafe", "lure", "kite", "reader", "pus", "mercury"]
            } else {
                &["newcomer", "arranger", "kite", "reader", "pus", "mercury"]
            };
            if !allowed.contains(&level.policy.as_str()) && !build_policy(&level.policy) {
                return Err(format!(
                    "level {}: policy {:?} is not supported here (only {allowed:?})",
                    level.id, level.policy
                ));
            }
            if let Some(m) = &level.map {
                let problem = if m.rooms == 0 {
                    Some("map.rooms must be at least 1".to_string())
                } else if m.room_side.0 < 3 || m.room_side.0 > m.room_side.1 {
                    Some("map.room_side must be [min, max] with 3 <= min <= max".into())
                } else if m.enemies_per_room.0 > m.enemies_per_room.1 {
                    Some("map.enemies_per_room must be [min, max] with min <= max".into())
                } else if m.corridor < 0 || m.vision < 1 {
                    Some("map.corridor must be >= 0 and map.vision >= 1".into())
                } else if m.enemy_kinds.is_empty() {
                    Some("map.enemy_kinds must name at least one enemy".into())
                } else if m.blockers.is_empty() != (m.blockers_per_level == 0) {
                    Some("map.blockers and map.blockers_per_level go together".into())
                } else if m.hunter.as_ref().is_some_and(|h| h.scent_every == 0) {
                    Some("map.hunter.scent_every must be at least 1".into())
                } else if m.hunter.as_ref().is_some_and(|h| {
                    design
                        .enemies
                        .iter()
                        .any(|e| e.name == h.kind && e.speed < 1)
                }) {
                    Some("map.hunter: its kind must move (speed >= 1)".into())
                } else if !m.camp.is_empty() && m.rooms < 2 {
                    Some("map.camp needs at least 2 rooms".into())
                } else if m.river_liquids.is_empty() != (m.river == 0) {
                    Some("map.river and map.river_liquids go together".into())
                } else if m.spawner.as_ref().is_some_and(|s| {
                    s.kinds.is_empty() || s.every == 0 || s.max_alive == 0 || m.rooms < 3
                }) {
                    Some("map.spawner needs kinds, every >= 1, max_alive >= 1 and 3+ rooms".into())
                } else if m.surface_rooms > m.rooms || (m.surface_rooms == 0) != (m.sun_every == 0)
                {
                    Some("map.surface_rooms (<= rooms) and map.sun_every go together".into())
                } else {
                    m.enemy_kinds
                        .iter()
                        .chain(&m.blockers)
                        .chain(&m.camp)
                        .chain(m.hunter.as_ref().map(|h| &h.kind))
                        .chain(m.spawner.iter().flat_map(|s| &s.kinds))
                        .find(|k| !design.enemies.iter().any(|e| &e.name == *k))
                        .map(|k| format!("no enemy named {k}"))
                };
                if let Some(problem) = problem {
                    return Err(format!("level {:?}: {problem}", level.id));
                }
            }
            for stage in &level.stages {
                if let Stage::Fight { enemy, enemies } = stage {
                    if !design.enemies.iter().any(|e| &e.name == enemy) {
                        return Err(format!("level {}: no enemy named {enemy}", level.id));
                    }
                    if *enemies == 0 || *enemies > design.matrix.enemy_rows.len() {
                        return Err(format!(
                            "level {}: {enemies} {enemy} at once, matrix.enemy_rows allows 1..={}",
                            level.id,
                            design.matrix.enemy_rows.len()
                        ));
                    }
                }
            }
        }
        let rows = crate::ladder::rows(&design);
        for (i, &(level, p)) in rows.iter().enumerate() {
            let same = |&(l, q): &(&Level, &LadderPlace)| {
                q.difficulty == p.difficulty && l.policy == level.policy
            };
            let lowest = rows
                .iter()
                .filter(|r| same(r))
                .all(|r| r.1.level >= p.level);
            // A branch grows by at most one choice per level and never drops one.
            let grows = |parent: &str| {
                p.branch == parent
                    || (p.continues(parent)
                        && !p.branch[parent.len()..]
                            .trim_start_matches('-')
                            .contains('-'))
            };
            let problem = if level.map.is_none() {
                Some("ladder needs a map level".to_string())
            } else if !["E", "N", "H"].contains(&p.difficulty.as_str()) || p.level == 0 {
                Some("ladder.difficulty must be E, N or H and ladder.level >= 1".into())
            } else if !p.branch.is_empty() && p.branch.split('-').any(|c| c.is_empty()) {
                Some(format!("ladder.branch {:?} has an empty choice", p.branch))
            } else if rows[..i]
                .iter()
                .any(|r| same(r) && r.1.level == p.level && r.1.branch == p.branch)
            {
                Some("two rows share level, difficulty, policy and branch".into())
            } else if !lowest
                && crate::ladder::parent(&rows, i).is_none_or(|j| !grows(&rows[j].1.branch))
            {
                Some(format!(
                    "the level {} row this continues must have the same branch or one choice fewer",
                    p.level - 1
                ))
            } else {
                None
            };
            if let Some(problem) = problem {
                return Err(format!("level {:?}: {problem}", level.id));
            }
        }
        for t in &design.ladder.thresholds {
            if t.min.is_some() == t.max.is_some()
                || !["E", "N", "H"].contains(&t.difficulty.as_str())
                || !(["strafe", "lure", "kite", "reader", "pus", "mercury"]
                    .contains(&t.policy.as_str())
                    || build_policy(&t.policy))
            {
                return Err(format!(
                    "ladder.thresholds {} {}: difficulty E, N or H, a map policy, exactly one of min, max",
                    t.difficulty, t.policy
                ));
            }
        }
        Ok(design)
    }

    /// The design file shipped with the repository.
    pub fn bundled() -> Design {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/design.toml");
        Design::load(&path).expect("data/design.toml is valid")
    }

    pub fn liquid(&self, liquid: Liquid) -> &LiquidData {
        self.liquids
            .iter()
            .find(|l| l.name == liquid)
            .expect("every liquid is listed (checked in parse)")
    }

    pub fn enemy(&self, name: &str) -> &EnemyData {
        self.enemies
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("no enemy named {name} in data/design.toml"))
    }
}
