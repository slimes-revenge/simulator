//! The plain-text report (`design-guard.txt`, `docs/report-format.md`) for the balance
//! session: no timestamps, stable order, fixed columns, so two versions compare with `diff`.

use crate::level::LevelResult;
use crate::report::Report;
use std::fmt::Write;

/// `design_toml` is the design file the reports were built from; its numbers open the report.
/// `design_file` is the `--design` path when one was given.
pub fn render(
    design_toml: &str,
    design_file: Option<&str>,
    reports: &[Report],
    levels: &[LevelResult],
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
    if !levels.is_empty() {
        render_levels(levels, &mut out);
    }
    Ok(out)
}

fn render_levels(levels: &[LevelResult], out: &mut String) {
    *out += "\n[levels]\n";
    let _ = writeln!(
        out,
        "{:<14} {:>5} {:<8} {:<14} {:>7} {:>3} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7}",
        "level",
        "stage",
        "kind",
        "detail",
        "reached",
        "win",
        "before",
        "spent",
        "bitten",
        "status",
        "grown",
        "eaten",
        "drunk",
        "wasted",
        "after",
        "net"
    );
    for l in levels {
        for r in &l.rows {
            let win = r.win.map_or("-".to_string(), |w| format!("{w:.0}"));
            let _ = writeln!(
                out,
                "{:<14} {:>5} {:<8} {:<14} {:>7.0} {:>3} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>+7.2}",
                l.id,
                r.stage,
                r.kind,
                r.detail,
                r.reached,
                win,
                r.before,
                r.spent,
                r.bitten,
                r.status,
                r.grown,
                r.eaten,
                r.drunk,
                r.wasted,
                r.after,
                r.net
            );
        }
    }
    *out += "\n[level-ends]\n";
    let _ = writeln!(
        out,
        "{:<14} {:>3} {:>6} {:>5} {:>6}  stops",
        "level", "end", "min", "worst", "wasted"
    );
    for l in levels {
        let mut stops: Vec<(&String, &u64)> = l.stops.iter().collect();
        stops.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let stops = if stops.is_empty() {
            "-".to_string()
        } else {
            stops
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let _ = writeln!(
            out,
            "{:<14} {:>3.0} {:>6.2} {:>5} {:>6.2}  {stops}",
            l.id, l.end, l.min_body, l.worst_body, l.wasted
        );
    }
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
            for (i, item) in items.iter().enumerate() {
                // Keyed by `name` or `id` when the table has one, else by its 1-based position.
                let mut rest = item.as_table().cloned().unwrap_or_default();
                let key = match rest.remove("name").or_else(|| rest.remove("id")) {
                    Some(toml::Value::String(k)) => k,
                    _ => (i + 1).to_string(),
                };
                flatten(&format!("{prefix}.{key}"), &toml::Value::Table(rest), out);
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
