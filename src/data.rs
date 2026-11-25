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
