//! `simulator` — see `USAGE` and README.md.
//! Runs the matrix once per `matrix.action_points` value and writes the reports as one JSON
//! array for the site and one plain-text file for the balance session (`docs/report-format.md`).

use simulator::data::Design;
use simulator::report;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: simulator [--design PATH] [--out PATH] [--no-json] [--out-txt PATH|-]
                 [--only-builds ID,ID] [--only-enemies NAME,NAME]
                 [--trace BUILD ENCOUNTER SEED AP]";

fn main() -> ExitCode {
    let mut design: Option<String> = None;
    let mut out = Some(PathBuf::from("design-guard.json"));
    let mut out_txt = String::from("design-guard.txt");
    let mut only_builds: Option<Vec<String>> = None;
    let mut only_enemies: Option<Vec<String>> = None;
    let mut trace: Option<Vec<String>> = None;
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
                out = Some(v.into());
                2
            }
            ("--no-json", _) => {
                out = None;
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
            ("--trace", _) if args.len() >= i + 5 => {
                trace = Some(args[i + 1..i + 5].to_vec());
                5
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
    let builds: Vec<report::Build> = report::builds(&d)
        .into_iter()
        .filter(|b| only_builds.as_ref().is_none_or(|ids| ids.contains(&b.id)))
        .collect();
    let encounters: Vec<report::Encounter> = report::encounters(&d)
        .into_iter()
        .filter(|e| {
            only_enemies
                .as_ref()
                .is_none_or(|names| names.contains(&e.enemy))
        })
        .collect();

    if let Some(t) = trace {
        let build = builds.iter().find(|b| b.id == t[0]);
        let encounter = encounters.iter().find(|e| e.id == t[1]);
        let (Some(b), Some(e), Ok(seed), Ok(ap)) = (build, encounter, t[2].parse(), t[3].parse())
        else {
            eprintln!("--trace needs a build id, an encounter id, a seed and an action count");
            return ExitCode::from(2);
        };
        for line in report::fight(&d, b, e, ap, seed).run_traced().1 {
            println!("{line}");
        }
        return ExitCode::SUCCESS;
    }

    let generated = utc_now();
    let commit = commit();
    let reports: Vec<report::Report> = d
        .matrix
        .action_points
        .iter()
        .map(|&ap| {
            report::run(
                &d,
                ap,
                builds.clone(),
                encounters.clone(),
                &generated,
                &commit,
            )
        })
        .collect();
    let txt = simulator::text::render(&text, design.as_deref(), &reports)
        .expect("the design file parsed above");
    if out_txt == "-" {
        print!("{txt}");
    } else if let Err(e) = std::fs::write(&out_txt, txt) {
        eprintln!("cannot write {out_txt}: {e}");
        return ExitCode::FAILURE;
    }
    if let Some(out) = out {
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
