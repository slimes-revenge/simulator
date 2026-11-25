//! The 20-level ladder: map levels tagged with `ladder = { level, difficulty, branch }`, played
//! two ways — each level from its own start (`level` mode) and as one run from the first level
//! up, the body carried from exit to entrance (`run` mode). Rules in `docs/ladder.md`.

use crate::data::{Design, LadderPlace, Level, Liquid, Threshold};
use crate::map::{self, MapLevelResult};
use rayon::prelude::*;

/// Per seed: the body that left a level's exit, `None` for a seed that died.
type Bodies = Vec<Option<Vec<Liquid>>>;

pub const MODES: [&str; 2] = ["level", "run"];

/// One ladder level in one mode.
#[derive(Clone, Debug)]
pub struct LadderRow {
    pub mode: &'static str,
    pub place: LadderPlace,
    pub policy: String,
    pub result: MapLevelResult,
}

/// How far a difficulty × policy × branch climbs under one threshold.
#[derive(Clone, Debug, PartialEq)]
pub struct SummaryRow {
    pub mode: &'static str,
    pub difficulty: String,
    pub policy: String,
    pub branch: String,
    pub rule: String,
    /// The last level of the unbroken passing streak from the lowest level.
    pub furthest: Option<u32>,
    /// The level that broke the streak.
    pub first_fail: Option<u32>,
    /// Its most frequent stop.
    pub cause: Option<String>,
}

pub(crate) fn rows(d: &Design) -> Vec<(&Level, &LadderPlace)> {
    d.levels
        .iter()
        .filter_map(|l| l.ladder.as_ref().map(|p| (l, p)))
        .collect()
}

fn same_ladder(a: (&Level, &LadderPlace), b: (&Level, &LadderPlace)) -> bool {
    a.1.difficulty == b.1.difficulty && a.0.policy == b.0.policy
}

/// The row at the previous level that `i` continues: the longest branch it extends.
pub(crate) fn parent(rows: &[(&Level, &LadderPlace)], i: usize) -> Option<usize> {
    let (_, p) = rows[i];
    (0..rows.len())
        .filter(|&j| same_ladder(rows[i], rows[j]) && rows[j].1.level + 1 == p.level)
        .filter(|&j| p.continues(&rows[j].1.branch))
        .max_by_key(|&j| rows[j].1.branch.len())
}

pub fn run(d: &Design) -> (Vec<LadderRow>, Vec<SummaryRow>) {
    let rows = rows(d);
    let seeds = d.matrix.seeds;
    let mut out: Vec<LadderRow> = rows
        .par_iter()
        .map(|&(level, place)| LadderRow {
            mode: "level",
            place: place.clone(),
            policy: level.policy.clone(),
            result: map::run(d, level),
        })
        .collect();

    // Run mode, level by level: each seed carries its body from the parent row's exit.
    let mut bodies: Vec<Bodies> = vec![Vec::new(); rows.len()];
    let mut levels: Vec<u32> = rows.iter().map(|r| r.1.level).collect();
    levels.sort();
    levels.dedup();
    for k in levels {
        let here: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].1.level == k).collect();
        let played: Vec<(usize, LadderRow, Bodies)> = here
            .par_iter()
            .map(|&i| {
                let (level, place) = rows[i];
                let spec = level.map.as_ref().expect("checked in parse");
                let ld = map::level_design(d, level);
                let start: Bodies = match parent(&rows, i) {
                    Some(j) => bodies[j].clone(),
                    None => vec![Some(map::start_body(level)); seeds as usize],
                };
                let runs: Vec<Option<_>> = start
                    .par_iter()
                    .enumerate()
                    .map(|(seed, body)| {
                        body.as_ref()
                            .map(|b| map::play(&ld, level, spec, b, seed as u64))
                    })
                    .collect();
                let next = runs
                    .iter()
                    .map(|r| {
                        r.as_ref()
                            .filter(|(f, _)| f.won)
                            .map(|(_, s)| s.body.clone())
                    })
                    .collect();
                let played: Vec<_> = runs.into_iter().flatten().collect();
                let row = LadderRow {
                    mode: "run",
                    place: place.clone(),
                    policy: level.policy.clone(),
                    result: map::summarize(level, &played, seeds),
                };
                (i, row, next)
            })
            .collect();
        for (i, row, next) in played {
            bodies[i] = next;
            out.push(row);
        }
    }
    let order = |r: &LadderRow| {
        (
            MODES.iter().position(|&m| m == r.mode),
            "ENH".find(&r.place.difficulty),
            r.policy.clone(),
            r.place.branch.clone(),
            r.place.level,
        )
    };
    out.sort_by_key(order);
    let summary = summarize(&rows, &out, &d.ladder.thresholds);
    (out, summary)
}

/// For each mode and threshold, one row per leaf (a ladder row no other row continues as its
/// parent): the climb along its parent chain.
fn summarize(
    rows: &[(&Level, &LadderPlace)],
    out: &[LadderRow],
    thresholds: &[Threshold],
) -> Vec<SummaryRow> {
    let is_parent: Vec<bool> = (0..rows.len())
        .map(|j| (0..rows.len()).any(|i| parent(rows, i) == Some(j)))
        .collect();
    let mut summary = Vec::new();
    for mode in MODES {
        let result = |i: usize| {
            let (l, p) = rows[i];
            &out.iter()
                .find(|r| r.mode == mode && &r.place == p && r.policy == l.policy)
                .expect("every row is played in every mode")
                .result
        };
        for t in thresholds {
            let mut leaves: Vec<usize> = (0..rows.len())
                .filter(|&i| !is_parent[i])
                .filter(|&i| rows[i].1.difficulty == t.difficulty && rows[i].0.policy == t.policy)
                .collect();
            leaves.sort_by_key(|&i| (rows[i].1.branch.clone(), rows[i].1.level));
            for leaf in leaves {
                let mut chain = vec![leaf];
                while let Some(j) = parent(rows, *chain.last().expect("not empty")) {
                    chain.push(j);
                }
                chain.reverse();
                let fail = chain.iter().copied().find(|&i| !t.meets(result(i).end));
                let furthest = chain
                    .iter()
                    .take_while(|&&i| t.meets(result(i).end))
                    .last()
                    .map(|&i| rows[i].1.level);
                summary.push(SummaryRow {
                    mode,
                    difficulty: t.difficulty.clone(),
                    policy: t.policy.clone(),
                    branch: rows[leaf].1.branch.clone(),
                    rule: t.rule(),
                    furthest,
                    first_fail: fail.map(|i| rows[i].1.level),
                    cause: fail.and_then(|i| {
                        result(i)
                            .stops
                            .iter()
                            .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                            .map(|(k, _)| k.clone())
                    }),
                });
            }
        }
    }
    summary
}
