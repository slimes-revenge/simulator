//! The design numbers, loaded from `data/design.toml`. Nothing tunable lives in code.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Liquid {
    Water,
    Oil,
    Poison,
    Acid,
    Blood,
    Lava,
    Mercury,
}

pub const ALL_LIQUIDS: [Liquid; 7] = [
    Liquid::Water,
    Liquid::Oil,
    Liquid::Poison,
    Liquid::Acid,
    Liquid::Blood,
    Liquid::Lava,
    Liquid::Mercury,
];

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Personality {
    /// Chases and attacks the slime it sees.
    Aggressive,
    /// Ignores the slime until hit, then behaves as Aggressive (sticky aggro).
    Passive,
    /// Flees the slime it sees; attacks once only when aggroed and adjacent.
    Cowardly,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trait {
    /// Heals `statuses.vampirism_heal` HP when it receives Blood or strikes a Blood unit off the slime.
    Vampirism,
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
}

/// `[ladder]`: what counts as passing a ladder level, per difficulty and policy.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct LadderRules {
    #[serde(default)]
    pub thresholds: Vec<Threshold>,
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
    pub pursuit_memory_turns: i32,
    pub max_rounds: u32,
    /// П15, an agreed design rule: at 1 unit a strike hits and spends nothing. The game does
    /// not implement it yet (its last unit cannot be spent); `false` models the game today.
    pub last_drop_strike: bool,
    /// Spit range (Chebyshev). Only 1 is modelled: spit replaces a strike on a neighbour.
    pub spit_range: i32,
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
    pub instability_speed: i32,
    pub vampirism_heal: i32,
    pub regeneration_growth: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LiquidData {
    pub name: Liquid,
    pub power: i32,
    pub corrosion: i32,
}

#[derive(Deserialize, Clone, Debug)]
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
        if let Some(p) = policies
            .iter()
            .find(|p| !["newcomer", "arranger", "kite"].contains(&p.as_str()))
        {
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
                &["strafe", "lure", "kite"]
            } else {
                &["newcomer", "arranger", "kite"]
            };
            if !allowed.contains(&level.policy.as_str()) {
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
                } else if m.river_liquids.is_empty() != (m.river == 0) {
                    Some("map.river and map.river_liquids go together".into())
                } else if m.surface_rooms > m.rooms || (m.surface_rooms == 0) != (m.sun_every == 0)
                {
                    Some("map.surface_rooms (<= rooms) and map.sun_every go together".into())
                } else {
                    m.enemy_kinds
                        .iter()
                        .chain(&m.blockers)
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
                || !["strafe", "lure", "kite"].contains(&t.policy.as_str())
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
