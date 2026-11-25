//! `simulator [--design data/design.toml] [--out design-guard.json] [--out-txt design-guard.txt]`
//! Runs the matrix once per `matrix.action_points` value and writes the reports as one JSON
//! array for the site and one plain-text file for the balance session (`docs/report-format.md`).

use simulator::data::Design;
use simulator::report;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> ExitCode {
    let mut design = PathBuf::from("data/design.toml");
    let mut out = PathBuf::from("design-guard.json");
    let mut out_txt = PathBuf::from("design-guard.txt");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--design", Some(v)) => design = v.into(),
            ("--out", Some(v)) => out = v.into(),
            ("--out-txt", Some(v)) => out_txt = v.into(),
            _ => {
                eprintln!(
                    "usage: simulator [--design data/design.toml] [--out design-guard.json] [--out-txt design-guard.txt]"
                );
                return ExitCode::from(2);
            }
        }
    }
    let text = match std::fs::read_to_string(&design) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {}: {e}", design.display());
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
    let generated = utc_now();
    let commit = commit();
    let reports: Vec<report::Report> = d
        .matrix
        .action_points
        .iter()
        .map(|&ap| report::run(&d, ap, &generated, &commit))
        .collect();
    let json = serde_json::to_string_pretty(&reports).expect("reports serialize") + "\n";
    let txt = simulator::text::render(&text, &reports).expect("the design file parsed above");
    for (path, content) in [(&out, json), (&out_txt, txt)] {
        if let Err(e) = std::fs::write(path, content) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
        println!("wrote {}", path.display());
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
