//! Level economy: the slime's body carried through a level's stages, fight by fight.
//! Gained: eaten corpses and fountains. Lost: strikes, bites, status pulses. Spec agreed with
//! the balance session; format in `docs/report-format.md`.

use crate::data::{Design, Level, Liquid, Stage};
use crate::fight::Policy;
use crate::report::{Build, Encounter, fight};
use std::collections::BTreeMap;

/// One stage, averaged over the seeds that reached it.
#[derive(Clone, Debug, PartialEq)]
pub struct StageRow {
    pub stage: usize,
    pub kind: &'static str,
    /// `mushroom-2` for a fight, `Water x3` for a fountain.
    pub detail: String,
    /// Percent of seeds that reached this stage.
    pub reached: f64,
    /// Percent of those that won the fight; `None` for a fountain or a stage nobody reached.
    pub win: Option<f64>,
    pub before: f64,
    pub spent: f64,
    pub bitten: f64,
    pub status: f64,
    /// Units lost to Thorns after melee strikes.
    pub thorns: f64,
    /// Blood grown by Regeneration during the fight.
    pub grown: f64,
    /// Units the corpses offered (eaten + what did not fit).
    pub corpse: f64,
    pub eaten: f64,
    pub drunk: f64,
    /// Corpse or fountain units that did not fit into capacity.
    pub wasted: f64,
    pub after: f64,
    /// corpse + drunk + grown − spent − bitten − status − thorns: the stage's own yield, ignoring
    /// capacity. A fountain's undrunk rest stays out: a full slime gains nothing from it.
    pub net: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LevelResult {
    pub id: String,
    pub rows: Vec<StageRow>,
    /// Percent of seeds that finished every stage.
    pub end: f64,
    /// Average over seeds of the smallest body between stages (the start included).
    pub min_body: f64,
    /// The smallest body between stages in any seed.
    pub worst_body: usize,
    /// Average total wasted units per seed.
    pub wasted: f64,
    /// Where runs stopped: `stage<k>:<cause>` → seeds.
    pub stops: BTreeMap<String, u64>,
}

#[derive(Default, Clone, Copy)]
struct Record {
    won: bool,
    before: usize,
    spent: usize,
    bitten: usize,
    status: usize,
    thorns: usize,
    grown: usize,
    corpse: usize,
    eaten: usize,
    drunk: usize,
    wasted: usize,
    after: usize,
}

pub fn run(d: &Design, level: &Level) -> LevelResult {
    // The level's capacity bounds the body in fights too (Regeneration, start).
    let mut ld = d.clone();
    ld.slime.capacity = level.capacity;
    let start = level
        .start
        .clone()
        .unwrap_or_else(|| vec![Liquid::Water; level.capacity]);
    let mut records: Vec<Vec<Record>> = vec![Vec::new(); level.stages.len()];
    let (mut ends, mut min_sum, mut worst, mut wasted_sum) = (0, 0, usize::MAX, 0);
    let mut stops = BTreeMap::new();
    for seed in 0..d.matrix.seeds {
        let mut body = start.clone();
        let mut min_body = body.len();
        let mut wasted = 0;
        let mut finished = true;
        for (k, stage) in level.stages.iter().enumerate() {
            let mut r = Record {
                before: body.len(),
                won: true,
                ..Record::default()
            };
            match stage {
                Stage::Fight { enemy, enemies } => {
                    let build = Build {
                        id: String::new(),
                        name: String::new(),
                        units: BTreeMap::new(),
                        volume: body.clone(),
                    };
                    let encounter = Encounter {
                        id: String::new(),
                        enemy: enemy.clone(),
                        enemies: *enemies,
                    };
                    let stage_seed = seed * 1000 + k as u64;
                    let mut f = fight(&ld, &build, &encounter, level.action_points, stage_seed);
                    f.policy = Policy::from_name(&level.policy, d);
                    let (result, volume) = f.run_with_volume();
                    r.won = result.won;
                    r.spent = result.units_spent;
                    r.bitten = result.units_bitten;
                    r.status = result.units_status;
                    r.thorns = result.units_thorns;
                    r.grown = result.units_grown;
                    // A death leaves nothing; a timeout leaves what the slime still had.
                    body = volume;
                    if result.won {
                        for _ in 0..*enemies {
                            for &unit in &d.enemy(enemy).volume {
                                r.corpse += 1;
                                if body.len() < level.capacity {
                                    body.push(unit);
                                    r.eaten += 1;
                                } else {
                                    r.wasted += 1;
                                }
                            }
                        }
                    } else {
                        let cause = result.cause.unwrap_or_else(|| "unknown".into());
                        *stops.entry(format!("stage{}:{cause}", k + 1)).or_insert(0) += 1;
                    }
                }
                Stage::Fountain { liquid, units } => {
                    r.drunk = (*units).min(level.capacity - body.len());
                    r.wasted = units - r.drunk;
                    body.extend(std::iter::repeat_n(*liquid, r.drunk));
                }
            }
            r.after = body.len();
            wasted += r.wasted;
            records[k].push(r);
            if !r.won {
                finished = false;
                break;
            }
            min_body = min_body.min(body.len());
        }
        ends += usize::from(finished);
        min_sum += min_body;
        worst = worst.min(min_body);
        wasted_sum += wasted;
    }
    let seeds = d.matrix.seeds as f64;
    let rows = level
        .stages
        .iter()
        .zip(&records)
        .enumerate()
        .map(|(k, (stage, recs))| row(k + 1, stage, recs, seeds))
        .collect();
    LevelResult {
        id: level.id.clone(),
        rows,
        end: round2(100.0 * ends as f64 / seeds),
        min_body: round2(min_sum as f64 / seeds),
        worst_body: worst,
        wasted: round2(wasted_sum as f64 / seeds),
        stops,
    }
}

fn row(stage: usize, s: &Stage, recs: &[Record], seeds: f64) -> StageRow {
    let n = recs.len().max(1) as f64;
    let avg = |f: fn(&Record) -> usize| round2(recs.iter().map(f).sum::<usize>() as f64 / n);
    let (kind, detail, win) = match s {
        Stage::Fight { enemy, enemies } => (
            "fight",
            format!("{}-{enemies}", enemy.to_lowercase()),
            (!recs.is_empty())
                .then(|| round2(100.0 * recs.iter().filter(|r| r.won).count() as f64 / n)),
        ),
        Stage::Fountain { liquid, units } => ("fountain", format!("{liquid:?} x{units}"), None),
    };
    let net = |r: &Record| {
        (r.corpse + r.drunk + r.grown) as f64 - (r.spent + r.bitten + r.status + r.thorns) as f64
    };
    StageRow {
        stage,
        kind,
        detail,
        reached: round2(100.0 * recs.len() as f64 / seeds),
        win,
        before: avg(|r| r.before),
        spent: avg(|r| r.spent),
        bitten: avg(|r| r.bitten),
        status: avg(|r| r.status),
        thorns: avg(|r| r.thorns),
        grown: avg(|r| r.grown),
        corpse: avg(|r| r.corpse),
        eaten: avg(|r| r.eaten),
        drunk: avg(|r| r.drunk),
        wasted: avg(|r| r.wasted),
        after: avg(|r| r.after),
        net: round2(recs.iter().map(net).sum::<f64>() / n),
    }
}

/// Two decimals; `+ 0.0` turns -0.0 into 0.0 so it never prints as "-0.00".
fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0 + 0.0
}
