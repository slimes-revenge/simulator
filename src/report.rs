//! The build × encounter matrix and its JSON report (`docs/report-format.md`).

use crate::data::{Design, Liquid};
use crate::fight::{Fight, FightResult};
use serde::Serialize;
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;
pub const POLICY: &str = "face-tank";

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Build {
    pub id: String,
    pub name: String,
    /// Units per liquid.
    pub units: BTreeMap<Liquid, usize>,
    /// The volume in order: strikes take the first unit of a kind, hits pop the last.
    #[serde(skip)]
    pub volume: Vec<Liquid>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Encounter {
    pub id: String,
    pub enemy: String,
    pub enemies: usize,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CellResult {
    pub build: String,
    pub encounter: String,
    pub enemy: String,
    pub enemies: usize,
    pub win_rate: f64,
    pub avg_rounds: f64,
    pub avg_units_left: f64,
    pub avg_units_spent: f64,
    /// Units lost to enemy bites.
    pub avg_units_bitten: f64,
    /// Units lost to the slime's own status pulses.
    pub avg_units_status: f64,
    pub avg_enemies_killed: f64,
    /// The most common cause of a loss; `None` when every fight was won.
    pub cause: Option<String>,
    /// Every cause of a loss with the number of fights it ended.
    pub causes: BTreeMap<String, u64>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: u32,
    pub generated_utc: String,
    pub commit: String,
    pub policy: String,
    pub actions_per_round: u32,
    pub seeds: u64,
    pub builds: Vec<Build>,
    pub encounters: Vec<Encounter>,
    pub cells: Vec<CellResult>,
}

/// The starting mix and one pure build per liquid.
pub fn builds(d: &Design) -> Vec<Build> {
    let mut builds = vec![build("start", "Starting mix", d.slime.start.clone())];
    for l in &d.liquids {
        let name = format!("{:?}", l.name);
        let volume = vec![l.name; d.matrix.pure_build_units];
        builds.push(build(
            &name.to_lowercase(),
            &format!("{} {name}", volume.len()),
            volume,
        ));
    }
    builds
}

fn build(id: &str, name: &str, volume: Vec<Liquid>) -> Build {
    let mut units = BTreeMap::new();
    for &l in &volume {
        *units.entry(l).or_insert(0) += 1;
    }
    Build {
        id: id.into(),
        name: name.into(),
        units,
        volume,
    }
}

/// Every enemy of the roster, `matrix.enemy_counts` at a time.
pub fn encounters(d: &Design) -> Vec<Encounter> {
    d.enemies
        .iter()
        .flat_map(|e| {
            d.matrix.enemy_counts.iter().map(|&n| Encounter {
                id: format!("{}-{n}", e.name.to_lowercase()),
                enemy: e.name.clone(),
                enemies: n,
            })
        })
        .collect()
}

/// One fight: the slime at `matrix.slime_cell`, the enemies in a column east of it.
pub fn fight<'a>(
    d: &'a Design,
    build: &Build,
    encounter: &Encounter,
    actions_per_round: u32,
    seed: u64,
) -> Fight<'a> {
    let (x, _) = d.matrix.slime_cell;
    let enemies: Vec<(&str, (i32, i32))> = d.matrix.enemy_rows[..encounter.enemies]
        .iter()
        .map(|&y| {
            (
                encounter.enemy.as_str(),
                (x + d.matrix.enemy_column_offset, y),
            )
        })
        .collect();
    Fight::new(
        d,
        seed,
        actions_per_round,
        build.volume.clone(),
        d.matrix.slime_cell,
        &enemies,
    )
}

pub fn play(
    d: &Design,
    build: &Build,
    encounter: &Encounter,
    actions_per_round: u32,
    seed: u64,
) -> FightResult {
    fight(d, build, encounter, actions_per_round, seed).run()
}

/// Every build × encounter given (normally `builds(d)` × `encounters(d)`), `matrix.seeds` each.
pub fn run(
    d: &Design,
    actions_per_round: u32,
    builds: Vec<Build>,
    encounters: Vec<Encounter>,
    generated_utc: &str,
    commit: &str,
) -> Report {
    let mut cells = Vec::new();
    for b in &builds {
        for e in &encounters {
            let results: Vec<FightResult> = (0..d.matrix.seeds)
                .map(|seed| play(d, b, e, actions_per_round, seed))
                .collect();
            cells.push(summarize(b, e, &results));
        }
    }
    Report {
        schema_version: SCHEMA_VERSION,
        generated_utc: generated_utc.into(),
        commit: commit.into(),
        policy: POLICY.into(),
        actions_per_round,
        seeds: d.matrix.seeds,
        builds,
        encounters,
        cells,
    }
}

fn summarize(b: &Build, e: &Encounter, results: &[FightResult]) -> CellResult {
    let n = results.len() as f64;
    let avg = |f: fn(&FightResult) -> f64| round2(results.iter().map(f).sum::<f64>() / n);
    let mut causes = BTreeMap::new();
    for cause in results.iter().filter_map(|r| r.cause.clone()) {
        *causes.entry(cause).or_insert(0) += 1;
    }
    // Most common; ties go to the alphabetically first cause.
    let cause = causes
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        .map(|(c, _)| c.clone());
    CellResult {
        build: b.id.clone(),
        encounter: e.id.clone(),
        enemy: e.enemy.clone(),
        enemies: e.enemies,
        win_rate: avg(|r| if r.won { 1.0 } else { 0.0 }),
        avg_rounds: avg(|r| r.rounds as f64),
        avg_units_left: avg(|r| r.units_left as f64),
        avg_units_spent: avg(|r| r.units_spent as f64),
        avg_units_bitten: avg(|r| r.units_bitten as f64),
        avg_units_status: avg(|r| r.units_status as f64),
        avg_enemies_killed: avg(|r| r.enemies_killed as f64),
        cause,
        causes,
    }
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
