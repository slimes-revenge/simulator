//! The plain-text report (`design-guard.txt`, `docs/report-format.md`) for the balance
//! session: no timestamps, stable order, fixed columns, so two versions compare with `diff`.

use crate::report::Report;
use std::fmt::Write;

/// `design_toml` is the design file the reports were built from; its numbers open the report.
/// `design_file` is the `--design` path when one was given.
pub fn render(
    design_toml: &str,
    design_file: Option<&str>,
    reports: &[Report],
) -> Result<String, String> {
    let design: toml::Table = toml::from_str(design_toml).map_err(|e| e.to_string())?;
    let mut out = String::from("# Slime's Revenge design guard — plain-text report, format 1\n");
    out += "\n[design]\n";
    if let Some(file) = design_file {
        let _ = writeln!(out, "design.file = {file}");
    }
    flatten("", &toml::Value::Table(design), &mut out);
    out += "\n[cells]\n";
    let _ = writeln!(
        out,
        "{:<2} {:<10} {:<10} {:<16} {:>3} {:>7} {:>6} {:>6} {:>6} {:>6} {:>6}  cause",
        "ap",
        "policy",
        "build",
        "encounter",
        "win",
        "rounds",
        "killed",
        "left",
        "spent",
        "bitten",
        "status"
    );
    for r in reports {
        for c in &r.cells {
            let mut causes: Vec<(&String, &u64)> = c.causes.iter().collect();
            causes.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let cause = if causes.is_empty() {
                "-".to_string()
            } else {
                causes
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let _ = writeln!(
                out,
                "{:<2} {:<10} {:<10} {:<16} {:>3} {:>7.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2}  {cause}",
                r.actions_per_round,
                r.policy,
                c.build,
                c.encounter,
                (c.win_rate * 100.0).round(),
                c.avg_rounds,
                c.avg_enemies_killed,
                c.avg_units_left,
                c.avg_units_spent,
                c.avg_units_bitten,
                c.avg_units_status,
            );
        }
    }
    Ok(out)
}

/// One `key = value` per number, in file order. Arrays of tables are keyed by their `name`
/// (`liquids.Lava.power`); arrays of values are joined by spaces.
fn flatten(prefix: &str, value: &toml::Value, out: &mut String) {
    match value {
        toml::Value::Table(t) => {
            for (k, v) in t {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, v, out);
            }
        }
        toml::Value::Array(items) if !items.is_empty() && items.iter().all(|i| i.is_table()) => {
            for item in items {
                let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                let mut rest = item.as_table().cloned().unwrap_or_default();
                rest.remove("name");
                flatten(&format!("{prefix}.{name}"), &toml::Value::Table(rest), out);
            }
        }
        toml::Value::Array(items) => {
            let joined: Vec<String> = items.iter().map(scalar).collect();
            let _ = writeln!(out, "{prefix} = {}", joined.join(" "));
        }
        v => {
            let _ = writeln!(out, "{prefix} = {}", scalar(v));
        }
    }
}

fn scalar(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
