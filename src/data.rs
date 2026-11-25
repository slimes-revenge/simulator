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
    /// Level runs for the body economy (`src/level.rs`); none is fine.
    #[serde(default)]
    pub levels: Vec<Level>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Rules {
    pub board_width: i32,
    pub board_height: i32,
    pub vision_range: i32,
    pub dominance_percent: usize,
    pub pursuit_memory_turns: i32,
    pub max_rounds: u32,
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
    pub policy: String,
    pub stages: Vec<Stage>,
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
        for (i, level) in design.levels.iter().enumerate() {
            let problem = if design.levels[..i].iter().any(|l| l.id == level.id) {
                Some("duplicate id")
            } else if level.id.is_empty() || level.id.len() > 14 {
                Some("id must be 1..=14 characters (it is a fixed-width column)")
            } else if level.capacity == 0 || level.action_points == 0 {
                Some("capacity and action_points must be at least 1")
            } else if level.stages.is_empty() || level.stages.len() >= 1000 {
                Some("stages must have 1..1000 entries")
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
            if level.policy != "face-tank" {
                return Err(format!(
                    "level {}: policy {:?} is not supported (only \"face-tank\")",
                    level.id, level.policy
                ));
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
