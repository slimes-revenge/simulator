//! The plain-text report (`design-guard.txt`, `docs/report-format.md`) for the balance
//! session: no timestamps, stable order, fixed columns, so two versions compare with `diff`.

use crate::ladder::{LadderRow, SummaryRow};
use crate::level::LevelResult;
use crate::map::MapLevelResult;
use crate::report::Report;
use std::fmt::Write;

/// `design_toml` is the design file the reports were built from; its numbers open the report.
/// `design_file` is the `--design` path when one was given.
pub fn render(
    design_toml: &str,
    design_file: Option<&str>,
    reports: &[Report],
    levels: &[LevelResult],
    maps: &[MapLevelResult],
) -> Result<String, String> {
    let design: toml::Table = toml::from_str(design_toml).map_err(|e| e.to_string())?;
    let mut out = String::from("# Slime's Revenge design guard — plain-text report, format 4\n");
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
    render_economy(reports, &mut out);
    render_damage(reports, &mut out);
    if !levels.is_empty() {
        render_levels(levels, &mut out);
    }
    if !maps.is_empty() {
        render_maps(maps, &mut out);
    }
    Ok(out)
}

fn render_levels(levels: &[LevelResult], out: &mut String) {
    *out += "\n[levels]\n";
    let _ = writeln!(
        out,
        "{:<14} {:>5} {:<8} {:<14} {:>7} {:>3} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7}",
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
        "thorns",
        "grown",
        "corpse",
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
                "{:<14} {:>5} {:<8} {:<14} {:>7.0} {:>3} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>+7.2}",
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
                r.thorns,
                r.grown,
                r.corpse,
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
        let stops = stops_field(&l.stops);
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

/// `Water:2.00,Lava:1.00`, or `-` when empty. Keys come in the map's order.
fn by<K: std::fmt::Debug>(map: &std::collections::BTreeMap<K, f64>) -> String {
    if map.is_empty() {
        return "-".into();
    }
    map.iter()
        .map(|(k, v)| format!("{}:{v:.2}", format!("{k:?}").trim_matches('"')))
        .collect::<Vec<_>>()
        .join(",")
}

fn key(r: &Report, c: &crate::report::CellResult) -> String {
    format!(
        "{:<2} {:<10} {:<10} {:<16}",
        r.actions_per_round, r.policy, c.build, c.encounter
    )
}

fn render_economy(reports: &[Report], out: &mut String) {
    *out += "\n[economy]\n";
    let _ = writeln!(
        out,
        "{:<2} {:<10} {:<10} {:<16} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7}  by",
        "ap",
        "policy",
        "build",
        "encounter",
        "spent",
        "bitten",
        "status",
        "thorns",
        "grown",
        "swaps",
        "net"
    );
    for r in reports {
        for c in &r.cells {
            let _ = writeln!(
                out,
                "{} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>+7.2}  spent={} bitten={} status={}",
                key(r, c),
                c.avg_units_spent,
                c.avg_units_bitten,
                c.avg_units_status,
                c.avg_units_thorns,
                c.avg_units_grown,
                c.avg_swaps,
                c.avg_net,
                by(&c.spent_by),
                by(&c.bitten_by),
                by(&c.status_lost_by)
            );
        }
    }
}

fn render_damage(reports: &[Report], out: &mut String) {
    *out += "\n[damage]\n";
    let _ = writeln!(
        out,
        "{:<2} {:<10} {:<10} {:<16} {:>6} {:>6} {:>6} {:>8} {:>6} {:>8}  status_by",
        "ap",
        "policy",
        "build",
        "encounter",
        "strike",
        "status",
        "retal",
        "overkill",
        "doomed",
        "per_unit"
    );
    for r in reports {
        for c in &r.cells {
            let per_unit = c
                .dmg_per_unit
                .map_or("-".to_string(), |v| format!("{v:.2}"));
            let _ = writeln!(
                out,
                "{} {:>6.2} {:>6.2} {:>6.2} {:>8.2} {:>6.2} {:>8}  {}",
                key(r, c),
                c.avg_strike_dmg,
                c.avg_status_dmg,
                c.avg_retaliation_dmg,
                c.avg_overkill,
                c.avg_doomed_strikes,
                per_unit,
                by(&c.status_dmg_by)
            );
        }
    }
}

fn stops_field(stops: &std::collections::BTreeMap<String, u64>) -> String {
    let mut stops: Vec<(&String, &u64)> = stops.iter().collect();
    stops.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    if stops.is_empty() {
        return "-".into();
    }
    stops
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_maps(maps: &[MapLevelResult], out: &mut String) {
    *out += "\n[map-levels]\n";
    let _ = writeln!(
        out,
        "{:<14} {:<10} {:>3} {:>5} {:>7} {:>6} {:>8} {:>8} {:>8} {:>9} {:>13} {:>11} {:>10} {:>9} {:>10} {:>6} {:>9} {:>9}",
        "level",
        "policy",
        "end",
        "rooms",
        "rounds",
        "fights",
        "engaged0",
        "engaged1",
        "engaged2",
        "engaged3+",
        "net_per_fight",
        "hunter",
        "hunter_net",
        "reactions",
        "react_loss",
        "stones",
        "elem_plus",
        "elem_miss"
    );
    for m in maps {
        let net = m
            .net_per_fight
            .map_or("-".to_string(), |v| format!("{v:+.2}"));
        let _ = writeln!(
            out,
            "{:<14} {:<10} {:>3.0} {:>5.2} {:>7.2} {:>6.2} {:>8.2} {:>8.2} {:>8.2} {:>9.2} {:>13} {:>11} {:>10} {:>9.2} {:>10.2} {:>6.2} {:>9.2} {:>9.2}",
            m.id,
            m.policy,
            m.end,
            m.rooms,
            m.rounds,
            m.fights,
            m.engaged[0],
            m.engaged[1],
            m.engaged[2],
            m.engaged[3],
            net,
            hunter_field(m),
            signed(m.hunter_net),
            m.reactions,
            m.react_loss,
            m.stones,
            m.elem_plus,
            m.elem_miss
        );
    }
    *out += "\n[map-level-ends]\n";
    let _ = writeln!(
        out,
        "{:<14} {:>3} {:>6} {:>5}  stops",
        "level", "end", "min", "worst"
    );
    for m in maps {
        let _ = writeln!(
            out,
            "{:<14} {:>3.0} {:>6.2} {:>5}  {}",
            m.id,
            m.end,
            m.min_body,
            m.worst_body,
            stops_field(&m.stops)
        );
    }
}

/// `[ladder]` and `[ladder-summary]`, appended after the other sections when the design has
/// ladder rows.
pub fn render_ladder(rows: &[LadderRow], summary: &[SummaryRow]) -> String {
    let mut out = String::from("\n[ladder]\n");
    let branch = |b: &str| {
        if b.is_empty() {
            "-".to_string()
        } else {
            b.to_string()
        }
    };
    let _ = writeln!(
        out,
        "{:<5} {:<10} {:<8} {:<24} {:>5} {:>3} {:>6} {:>13} {:>9} {:>8} {:>8} {:>11} {:>10} {:>9} {:>10} {:>6} {:>9} {:>9}  stops",
        "mode",
        "difficulty",
        "policy",
        "branch",
        "level",
        "end",
        "fights",
        "net_per_fight",
        "engaged2+",
        "min_body",
        "body_out",
        "hunter",
        "hunter_net",
        "reactions",
        "react_loss",
        "stones",
        "elem_plus",
        "elem_miss"
    );
    for r in rows {
        let m = &r.result;
        let net = m
            .net_per_fight
            .map_or("-".to_string(), |v| format!("{v:+.2}"));
        let _ = writeln!(
            out,
            "{:<5} {:<10} {:<8} {:<24} {:>5} {:>3.0} {:>6.2} {:>13} {:>9.2} {:>8.2} {:>8} {:>11} {:>10} {:>9.2} {:>10.2} {:>6.2} {:>9.2} {:>9.2}  {}",
            r.mode,
            r.place.difficulty,
            r.policy,
            branch(&r.place.branch),
            r.place.level,
            m.end,
            m.fights,
            net,
            m.engaged[2] + m.engaged[3],
            m.min_body,
            m.body_out.map_or("-".to_string(), |v| format!("{v:.2}")),
            hunter_field(m),
            signed(m.hunter_net),
            m.reactions,
            m.react_loss,
            m.stones,
            m.elem_plus,
            m.elem_miss,
            stops_field(&m.stops)
        );
    }
    out += "\n[ladder-summary]\n";
    let _ = writeln!(
        out,
        "{:<5} {:<10} {:<8} {:<24} {:<6} {:>14} {:>16}  cause",
        "mode", "difficulty", "policy", "branch", "rule", "furthest_level", "first_fail_level"
    );
    let level = |l: Option<u32>| l.map_or("-".to_string(), |l| l.to_string());
    for s in summary {
        let _ = writeln!(
            out,
            "{:<5} {:<10} {:<8} {:<24} {:<6} {:>14} {:>16}  {}",
            s.mode,
            s.difficulty,
            s.policy,
            branch(&s.branch),
            s.rule,
            level(s.furthest),
            level(s.first_fail),
            s.cause.as_deref().unwrap_or("-")
        );
    }
    out
}

/// `killed/alive/killed_slime` percents of the played seeds, `-` without a hunter.
fn hunter_field(m: &MapLevelResult) -> String {
    m.hunter
        .map_or("-".to_string(), |[k, a, s]| format!("{k:.0}/{a:.0}/{s:.0}"))
}

fn signed(v: Option<f64>) -> String {
    v.map_or("-".to_string(), |v| format!("{v:+.2}"))
}
