//! `simulator` — see `USAGE` and README.md.
//! Runs the matrix once per `matrix.action_points` value and writes the reports as one JSON
//! array for the site and one plain-text file for the balance session (`docs/report-format.md`).

use rayon::prelude::*;
use simulator::data::Design;
use simulator::report;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: simulator [--design PATH] [--out PATH] [--no-json] [--out-txt PATH|-]
                 [--only-builds ID,ID] [--only-enemies NAME,NAME]
                 [--trace BUILD ENCOUNTER SEED AP [--policy NAME]] [--trace-map LEVEL SEED]";

fn main() -> ExitCode {
    let mut design: Option<String> = None;
    let mut out = PathBuf::from("design-guard.json");
    let mut no_json = false;
    let mut out_txt = String::from("design-guard.txt");
    let mut only_builds: Option<Vec<String>> = None;
    let mut only_enemies: Option<Vec<String>> = None;
    let mut trace: Option<Vec<String>> = None;
    let mut trace_map: Option<(String, String)> = None;
    let mut trace_policy = String::from("newcomer");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let value = |n: usize| args.get(i + n).cloned();
        let list = |v: String| v.split(',').map(String::from).collect::<Vec<_>>();
        let taken = match (args[i].as_str(), value(1)) {
            ("--design", Some(v)) => {
                design = Some(v);
                2
            }
            ("--out", Some(v)) => {
                out = v.into();
                2
            }
            ("--no-json", _) => {
                no_json = true;
                1
            }
            ("--out-txt", Some(v)) => {
                out_txt = v;
                2
            }
            ("--only-builds", Some(v)) => {
                only_builds = Some(list(v));
                2
            }
            ("--only-enemies", Some(v)) => {
                only_enemies = Some(list(v));
                2
            }
            ("--trace", _)
                if args.len() >= i + 5
                    && !args[i + 1..i + 5].iter().any(|a| a.starts_with("--")) =>
            {
                trace = Some(args[i + 1..i + 5].to_vec());
                5
            }
            ("--policy", Some(p)) if ["newcomer", "arranger", "kite"].contains(&p.as_str()) => {
                trace_policy = p;
                2
            }
            ("--trace-map", Some(level)) if args.len() >= i + 3 => {
                trace_map = Some((level, args[i + 2].clone()));
                3
            }
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        };
        i += taken;
    }
    let design_path = design.clone().unwrap_or_else(|| "data/design.toml".into());
    let text = match std::fs::read_to_string(&design_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {design_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let d = match Design::parse(&text) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let all_builds = report::builds(&d);
    let all_encounters = report::encounters(&d);
    let build_ids: Vec<String> = all_builds.iter().map(|b| b.id.clone()).collect();
    let enemy_names: Vec<String> = d.enemies.iter().map(|e| e.name.clone()).collect();
    for (given, valid, flag) in [
        (&only_builds, &build_ids, "--only-builds"),
        (&only_enemies, &enemy_names, "--only-enemies"),
    ] {
        if let Some(bad) = given.iter().flatten().find(|g| !valid.contains(g)) {
            eprintln!("{flag}: unknown {bad:?}; valid: {}", valid.join(","));
            return ExitCode::from(2);
        }
    }
    let builds: Vec<report::Build> = all_builds
        .iter()
        .filter(|b| only_builds.as_ref().is_none_or(|ids| ids.contains(&b.id)))
        .cloned()
        .collect();
    let encounters: Vec<report::Encounter> = all_encounters
        .iter()
        .filter(|e| {
            only_enemies
                .as_ref()
                .is_none_or(|names| names.contains(&e.enemy))
        })
        .cloned()
        .collect();

    if let Some((id, seed)) = trace_map {
        let level = d.levels.iter().find(|l| l.id == id && l.map.is_some());
        let (Some(level), Ok(seed)) = (level, seed.parse::<u64>()) else {
            eprintln!("--trace-map needs a map level id and a seed");
            return ExitCode::from(2);
        };
        let spec = level.map.as_ref().expect("a map level");
        let mut ld = d.clone();
        ld.slime.capacity = level.capacity;
        let body = level
            .start
            .clone()
            .unwrap_or_else(|| vec![simulator::data::Liquid::Water; level.capacity]);
        print!(
            "{}",
            simulator::map::ascii(&simulator::map::generate(spec, seed))
        );
        for line in simulator::map::fight(&ld, level, spec, &body, seed)
            .run_traced()
            .1
        {
            println!("{line}");
        }
        return ExitCode::SUCCESS;
    }

    if let Some(t) = trace {
        let build = all_builds.iter().find(|b| b.id == t[0]);
        let encounter = all_encounters.iter().find(|e| e.id == t[1]);
        let (Some(b), Some(e), Ok(seed), Ok(ap)) = (build, encounter, t[2].parse(), t[3].parse())
        else {
            eprintln!("--trace needs a build id, an encounter id, a seed and an action count");
            return ExitCode::from(2);
        };
        let mut f = report::fight(&d, b, e, ap, seed);
        f.policy = simulator::fight::Policy::from_name(&trace_policy, &d);
        for line in f.run_traced().1 {
            println!("{line}");
        }
        return ExitCode::SUCCESS;
    }

    let generated = utc_now();
    let commit = commit();
    let mut action_points = d.matrix.action_points.clone();
    action_points.sort();
    action_points.dedup();
    // Sorted by actions per round, then policy in `matrix.policies` order.
    let reports: Vec<report::Report> = action_points
        .iter()
        .flat_map(|&ap| d.matrix.policies.iter().map(move |p| (ap, p)))
        .map(|(ap, policy)| {
            report::run_policy(
                &d,
                ap,
                policy,
                builds.clone(),
                encounters.clone(),
                &generated,
                &commit,
            )
        })
        .collect();
    // Levels run in parallel; `collect` keeps the file order, so the report is unchanged.
    let levels: Vec<simulator::level::LevelResult> = d
        .levels
        .par_iter()
        .filter(|l| l.map.is_none())
        .map(|l| simulator::level::run(&d, l))
        .collect();
    let maps: Vec<simulator::map::MapLevelResult> = d
        .levels
        .par_iter()
        .filter(|l| l.map.is_some())
        .map(|l| simulator::map::run(&d, l))
        .collect();
    let txt = simulator::text::render(&text, design.as_deref(), &reports, &levels, &maps)
        .expect("the design file parsed above");
    if out_txt == "-" {
        print!("{txt}");
    } else if let Err(e) = std::fs::write(&out_txt, txt) {
        eprintln!("cannot write {out_txt}: {e}");
        return ExitCode::FAILURE;
    }
    if !no_json {
        let json = serde_json::to_string_pretty(&reports).expect("reports serialize") + "\n";
        if let Err(e) = std::fs::write(&out, json) {
            eprintln!("cannot write {}: {e}", out.display());
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

/// `GITHUB_SHA` in CI, else the checkout's HEAD, else "unknown".
fn commit() -> String {
    if let Ok(sha) = std::env::var("GITHUB_SHA") {
        return sha;
    }
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// Now as `YYYY-MM-DDTHH:MM:SSZ` (civil-from-days, no date crate needed).
fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after 1970")
        .as_secs() as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}
