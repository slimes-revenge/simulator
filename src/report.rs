//! The build × encounter matrix and its JSON report (`docs/report-format.md`).

use crate::data::{Design, Liquid};
use crate::fight::{Fight, FightResult, Policy};
use serde::Serialize;
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Build {
    pub id: String,
    pub name: String,
    /// Units per liquid.
    pub units: BTreeMap<Liquid, usize>,
    /// The volume from the centre to the edge: the last unit is the edge, spent and hit first.
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
    /// Units lost to Thorns after melee strikes.
    pub avg_units_thorns: f64,
    pub avg_enemies_killed: f64,
    /// The most common cause of a loss; `None` when every fight was won.
    pub cause: Option<String>,
    /// Every cause of a loss with the number of fights it ended.
    pub causes: BTreeMap<String, u64>,
    // The [economy] and [damage] slices: averages over all seeds.
    pub avg_units_grown: f64,
    /// grown − spent − bitten − status, from integer sums (not from the rounded averages).
    pub avg_net: f64,
    pub spent_by: BTreeMap<Liquid, f64>,
    pub bitten_by: BTreeMap<Liquid, f64>,
    pub status_lost_by: BTreeMap<String, f64>,
    pub avg_strike_dmg: f64,
    pub avg_status_dmg: f64,
    pub avg_retaliation_dmg: f64,
    pub status_dmg_by: BTreeMap<String, f64>,
    pub avg_overkill: f64,
    pub avg_doomed_strikes: f64,
    /// Actions spent swapping units of the body.
    pub avg_swaps: f64,
    /// (strike + status + retaliation damage) / units spent, over all seeds; `None` when
    /// nothing was spent.
    pub dmg_per_unit: Option<f64>,
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

/// `run_policy` with the `newcomer` policy.
pub fn run(
    d: &Design,
    actions_per_round: u32,
    builds: Vec<Build>,
    encounters: Vec<Encounter>,
    generated_utc: &str,
    commit: &str,
) -> Report {
    run_policy(
        d,
        actions_per_round,
        "newcomer",
        builds,
        encounters,
        generated_utc,
        commit,
    )
}

/// Every build × encounter given (normally `builds(d)` × `encounters(d)`), `matrix.seeds` each.
pub fn run_policy(
    d: &Design,
    actions_per_round: u32,
    policy: &str,
    builds: Vec<Build>,
    encounters: Vec<Encounter>,
    generated_utc: &str,
    commit: &str,
) -> Report {
    let p = Policy::from_name(policy, d);
    let mut cells = Vec::new();
    for b in &builds {
        for e in &encounters {
            let results: Vec<FightResult> = (0..d.matrix.seeds)
                .map(|seed| {
                    let mut f = fight(d, b, e, actions_per_round, seed);
                    f.policy = p;
                    f.run()
                })
                .collect();
            cells.push(summarize(b, e, &results));
        }
    }
    Report {
        schema_version: SCHEMA_VERSION,
        generated_utc: generated_utc.into(),
        commit: commit.into(),
        policy: policy.into(),
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
        avg_units_thorns: avg(|r| r.units_thorns as f64),
        avg_enemies_killed: avg(|r| r.enemies_killed as f64),
        cause,
        causes,
        avg_units_grown: avg(|r| r.units_grown as f64),
        avg_net: avg(|r| {
            r.units_grown as f64
                - (r.units_spent + r.units_bitten + r.units_status + r.units_thorns) as f64
        }) + 0.0,
        spent_by: avg_map(results, |r| &r.tally.spent_by),
        bitten_by: avg_map(results, |r| &r.tally.bitten_by),
        status_lost_by: named(avg_map(results, |r| &r.tally.status_lost_by)),
        avg_strike_dmg: avg(|r| r.tally.strike_dmg as f64),
        avg_status_dmg: avg(|r| r.tally.status_dmg as f64),
        avg_retaliation_dmg: avg(|r| r.tally.retaliation_dmg as f64),
        status_dmg_by: named(avg_map(results, |r| &r.tally.status_dmg_by)),
        avg_overkill: avg(|r| r.tally.overkill as f64),
        avg_doomed_strikes: avg(|r| r.tally.doomed_strikes as f64),
        avg_swaps: avg(|r| r.tally.swaps as f64),
        dmg_per_unit: {
            let t =
                |r: &FightResult| r.tally.strike_dmg + r.tally.status_dmg + r.tally.retaliation_dmg;
            let dmg: usize = results.iter().map(t).sum();
            let spent: usize = results.iter().map(|r| r.units_spent).sum();
            (spent > 0).then(|| round2(dmg as f64 / spent as f64))
        },
    }
}

/// Per-key average over all fights (a fight without the key counts as 0).
fn avg_map<K: Ord + Copy>(
    results: &[FightResult],
    map: impl Fn(&FightResult) -> &BTreeMap<K, usize>,
) -> BTreeMap<K, f64> {
    let mut sum: BTreeMap<K, usize> = BTreeMap::new();
    for r in results {
        for (&k, &v) in map(r) {
            *sum.entry(k).or_insert(0) += v;
        }
    }
    let n = results.len() as f64;
    sum.into_iter()
        .map(|(k, v)| (k, round2(v as f64 / n)))
        .collect()
}

fn named(map: BTreeMap<&'static str, f64>) -> BTreeMap<String, f64> {
    map.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
