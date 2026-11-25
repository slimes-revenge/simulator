//! Acceptance checks for the design guard (docs/design/design-guard.md, first version).

use simulator::data::{Design, Liquid};
use simulator::fight::{Fight, FightResult, Tally};
use simulator::report::{self, Build, Encounter};
use simulator::text;

fn design() -> Design {
    Design::bundled()
}

fn pure(d: &Design, liquid: Liquid) -> Build {
    report::builds(d)
        .into_iter()
        .find(|b| b.units.len() == 1 && b.units.contains_key(&liquid))
        .unwrap()
}

fn one(enemy: &str) -> Encounter {
    Encounter {
        id: format!("{}-1", enemy.to_lowercase()),
        enemy: enemy.into(),
        enemies: 1,
    }
}

/// A won fight against one enemy.
fn won(rounds: u32, left: usize, spent: usize, bitten: usize) -> FightResult {
    FightResult {
        won: true,
        rounds,
        units_left: left,
        units_spent: spent,
        units_bitten: bitten,
        units_status: 0,
        units_grown: 0,
        enemies_killed: 1,
        cause: None,
        tally: Tally::default(),
    }
}

/// The result without the slice breakdown, to compare with `won(...)`.
fn core(r: FightResult) -> FightResult {
    FightResult {
        tally: Tally::default(),
        ..r
    }
}

#[test]
fn same_seed_gives_the_same_fight() {
    let d = design();
    for b in report::builds(&d) {
        for e in report::encounters(&d) {
            for ap in [1, 2] {
                for seed in [0, 7] {
                    assert_eq!(
                        report::play(&d, &b, &e, ap, seed),
                        report::play(&d, &b, &e, ap, seed),
                        "{} vs {} ap {ap} seed {seed}",
                        b.id,
                        e.id
                    );
                }
            }
        }
    }
}

#[test]
fn seeds_change_fights_with_wandering_enemies() {
    // Passive bats wander until hit, so different seeds give different fights.
    let d = design();
    let b = pure(&d, Liquid::Water);
    let e = one("Bat");
    let rounds: std::collections::BTreeSet<u32> = (0..20)
        .map(|s| report::play(&d, &b, &e, 1, s).rounds)
        .collect();
    assert!(rounds.len() > 1, "{rounds:?}");
}

// Hand-checked: slime (1,5), Mushroom (6,5), 5 HP, Aggressive. Both close in one cell per round
// and meet after round 2. Round 3: Lava strike 5→4 HP + Burning; the Mushroom's turn: Burning
// 4→3, it bites (10→9→8 units counting the strike), Lava dominance retaliates with a second
// Burning. Round 4: strike 3→2 + third Burning; its turn: two Burnings pulse 2→1→0, dead.
#[test]
fn ten_lava_vs_one_mushroom_one_action() {
    let d = design();
    let r = report::play(&d, &pure(&d, Liquid::Lava), &one("Mushroom"), 1, 0);
    assert_eq!(core(r.clone()), won(4, 7, 2, 1));
}

// With two actions: round 1 two steps, the Mushroom steps (distance 2); round 2 step + strike
// (4 HP), Burning 4→3, bite, retaliation Burning; round 3 two strikes 3→2→1, its first
// Burning pulse kills it. 3 strikes, 1 bite: 6 left.
#[test]
fn ten_lava_vs_one_mushroom_two_actions() {
    let d = design();
    let r = report::play(&d, &pure(&d, Liquid::Lava), &one("Mushroom"), 2, 0);
    assert_eq!(core(r.clone()), won(3, 6, 3, 1));
}

// Water deals 1 per strike and nothing over time: from round 3 each round is one strike and
// one bite. Strikes at 10, 8, 6, 4, 2 units; the fifth kills it at round 7 with 1 unit left.
#[test]
fn ten_water_vs_one_mushroom_one_action() {
    let d = design();
    let r = report::play(&d, &pure(&d, Liquid::Water), &one("Mushroom"), 1, 0);
    assert_eq!(core(r.clone()), won(7, 1, 5, 4));
}

#[test]
fn blood_cannot_kill_a_vampire_bat() {
    // A Blood strike deals 1, then Vampirism heals 2: the bat stays at full health.
    let d = design();
    let mut f = Fight::new(
        &d,
        0,
        1,
        vec![Liquid::Blood; 10],
        (1, 5),
        &[("Bat", (2, 5))],
    );
    f.slime_strike(0, Liquid::Blood);
    assert_eq!(f.enemies[0].hit_points, 3);
    let cell = report::run(&d, 1, report::builds(&d), report::encounters(&d), "t", "c")
        .cells
        .into_iter()
        .find(|c| c.build == "blood" && c.encounter == "bat-1")
        .unwrap();
    assert_eq!(cell.win_rate, 0.0);
}

#[test]
fn oil_makes_the_next_lava_strike_double() {
    let d = design();
    let mut f = Fight::new(
        &d,
        0,
        1,
        vec![Liquid::Oil, Liquid::Lava, Liquid::Lava],
        (1, 5),
        &[("Mushroom", (2, 5))],
    );
    f.slime_strike(0, Liquid::Oil);
    assert_eq!(f.enemies[0].hit_points, 4);
    f.slime_strike(0, Liquid::Lava);
    assert_eq!(f.enemies[0].hit_points, 2);
}

#[test]
fn water_puts_out_burning_and_is_not_added() {
    let d = design();
    let mut f = Fight::new(
        &d,
        0,
        1,
        vec![Liquid::Lava, Liquid::Water, Liquid::Water],
        (1, 5),
        &[("Mushroom", (2, 5))],
    );
    f.slime_strike(0, Liquid::Lava);
    assert_eq!(f.enemies[0].statuses.len(), 1);
    f.slime_strike(0, Liquid::Water);
    assert!(f.enemies[0].statuses.is_empty());
}

#[test]
fn report_covers_every_build_and_encounter() {
    let d = design();
    let builds = report::builds(&d);
    let encounters = report::encounters(&d);
    assert_eq!(builds.len(), 1 + 7);
    assert_eq!(encounters.len(), 6 * 3);
    for ap in d.matrix.action_points.clone() {
        let r = report::run(&d, ap, builds.clone(), encounters.clone(), "t", "c");
        assert_eq!(r.cells.len(), builds.len() * encounters.len());
        for b in &builds {
            for e in &encounters {
                let n = r
                    .cells
                    .iter()
                    .filter(|c| c.build == b.id && c.encounter == e.id)
                    .count();
                assert_eq!(n, 1, "{} × {}", b.id, e.id);
            }
        }
    }
}

#[test]
fn a_basic_number_changes_the_report() {
    let d = design();
    let mut stronger = d.clone();
    stronger
        .liquids
        .iter_mut()
        .find(|l| l.name == Liquid::Water)
        .unwrap()
        .power = 2;
    let cell = |d: &Design| {
        report::run(d, 1, report::builds(d), report::encounters(d), "t", "c")
            .cells
            .into_iter()
            .find(|c| c.build == "water" && c.encounter == "mushroom-2")
            .unwrap()
    };
    assert_ne!(cell(&d), cell(&stronger));
}

#[test]
fn report_json_has_the_contract_fields() {
    let d = design();
    let json = serde_json::to_value(report::run(
        &d,
        2,
        report::builds(&d),
        report::encounters(&d),
        "2026-01-01T00:00:00Z",
        "abc",
    ))
    .unwrap();
    for key in [
        "schemaVersion",
        "generatedUtc",
        "commit",
        "policy",
        "actionsPerRound",
        "seeds",
        "builds",
        "encounters",
        "cells",
    ] {
        assert!(json.get(key).is_some(), "missing {key}");
    }
    assert_eq!(json["schemaVersion"], 1);
    let cell = &json["cells"][0];
    for key in [
        "build",
        "encounter",
        "enemy",
        "enemies",
        "winRate",
        "avgRounds",
        "avgUnitsLeft",
        "avgUnitsSpent",
        "cause",
    ] {
        assert!(cell.get(key).is_some(), "cell missing {key}");
    }
}

#[test]
fn the_body_ledger_adds_up_without_regeneration() {
    // Only Blood dominance grows the body; every other unit is left, spent, bitten or burned.
    let d = design();
    for b in report::builds(&d).iter().filter(|b| b.id != "blood") {
        for e in report::encounters(&d) {
            for ap in [1, 2] {
                let r = report::play(&d, b, &e, ap, 3);
                assert_eq!(
                    r.units_left + r.units_spent + r.units_bitten + r.units_status,
                    b.volume.len(),
                    "{} vs {} ap {ap}: {r:?}",
                    b.id,
                    e.id
                );
                assert!(r.enemies_killed <= e.enemies);
                assert_eq!(r.won, r.enemies_killed == e.enemies);
            }
        }
    }
}

#[test]
fn text_report_is_stable_and_has_no_run_details() {
    let d = design();
    let toml =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let reports = |when: &str, sha: &str| -> Vec<report::Report> {
        [1, 2]
            .into_iter()
            .map(|ap| {
                report::run(
                    &d,
                    ap,
                    report::builds(&d),
                    report::encounters(&d),
                    when,
                    sha,
                )
            })
            .collect()
    };
    let a = text::render(
        &toml,
        None,
        &reports("2026-01-01T00:00:00Z", "aaaa"),
        &[],
        &[],
    )
    .unwrap();
    let b = text::render(
        &toml,
        None,
        &reports("2027-02-02T00:00:00Z", "bbbb"),
        &[],
        &[],
    )
    .unwrap();
    assert_eq!(
        a, b,
        "the text report must not depend on the time or the commit"
    );
    assert!(!a.contains("design.file"));
    assert!(a.contains("\nliquids.Lava.power = 1\n"));
    assert!(a.contains("\nenemies.Mushroom.traits = \n"));
    assert!(a.contains("\nmatrix.enemy_rows = 5 4 6\n"));
    assert!(a.contains("\nslime.start = Water Water Oil Poison Acid Lava\n"));
    let with_file = text::render(&toml, Some("x.toml"), &reports("t", "c"), &[], &[]).unwrap();
    assert!(with_file.contains("\ndesign.file = x.toml\n"));

    let table: Vec<&str> = a
        .split("[cells]\n")
        .nth(1)
        .unwrap()
        .lines()
        .take_while(|l| !l.is_empty())
        .collect();
    let (header, cells) = (table[0], &table[1..]);
    assert_eq!(cells.len(), 2 * 8 * 18);
    for section in ["[economy]", "[damage]"] {
        let rows = a.split(section).nth(1).unwrap().lines().skip(2);
        assert_eq!(
            rows.take_while(|l| !l.is_empty()).count(),
            2 * 8 * 18,
            "{section}"
        );
    }
    assert_eq!(
        cells[0].split_whitespace().take(4).collect::<Vec<_>>(),
        ["1", "newcomer", "start", "bat-1"]
    );
    assert_eq!(
        cells
            .last()
            .unwrap()
            .split_whitespace()
            .take(4)
            .collect::<Vec<_>>(),
        ["2", "newcomer", "mercury", "eye-3"]
    );
    // Fixed columns: the cause starts under the header's "cause" in every line.
    let cause_at = header.find("cause").unwrap();
    for l in cells {
        assert_eq!(&l[cause_at - 2..cause_at], "  ", "{l}");
        assert_ne!(&l[cause_at..cause_at + 1], " ", "{l}");
    }
    let lava = cells
        .iter()
        .find(|l| {
            l.split_whitespace()
                .take(4)
                .eq(["1", "newcomer", "lava", "mushroom-1"])
        })
        .unwrap();
    assert_eq!(
        lava.split_whitespace().skip(4).collect::<Vec<_>>(),
        ["100", "4.00", "1.00", "7.00", "2.00", "1.00", "0.00", "-"]
    );
}

#[test]
fn trace_logs_the_fight_turn_by_turn() {
    let d = design();
    let (r, log) = report::fight(&d, &pure(&d, Liquid::Lava), &one("Mushroom"), 1, 0).run_traced();
    assert_eq!(core(r.clone()), won(4, 7, 2, 1));
    assert!(log.iter().any(|l| l == "round 4"));
    assert!(
        log.iter()
            .any(|l| l.starts_with("Slime strikes Mushroom#1 with Lava: HP 5->4"))
    );
    assert!(
        log.iter()
            .any(|l| l.starts_with("Mushroom#1 pulse Burning: HP 1->0"))
    );
    assert!(log.last().unwrap().starts_with("result:"));
}

fn level_design(stages: &str) -> Design {
    let mut text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += &format!(
        "[[levels]]\nid = \"t\"\ncapacity = 10\nstart = [{}]\naction_points = 1\npolicy = \"newcomer\"\nstages = [{stages}]\n",
        ["\"Lava\""; 10].join(", ")
    );
    Design::parse(&text).unwrap()
}

// 10 Lava vs 1 Mushroom (hand-checked above: 2 strikes, 1 bite, 7 left), then the slime eats
// the Mushroom's 1 Water: 8. The fountain of 5 Water fills it to 10 (2 drunk, 3 wasted).
#[test]
fn level_carries_the_body_from_fight_to_fountain() {
    let d = level_design(
        r#"{ kind = "fight", enemy = "Mushroom", enemies = 1 },
           { kind = "fountain", liquid = "Water", units = 5 }"#,
    );
    let l = simulator::level::run(&d, &d.levels[0]);
    let (fight, fountain) = (&l.rows[0], &l.rows[1]);
    assert_eq!((fight.reached, fight.win), (100.0, Some(100.0)));
    assert_eq!(
        (
            fight.before,
            fight.spent,
            fight.bitten,
            fight.eaten,
            fight.after,
            fight.net
        ),
        (10.0, 2.0, 1.0, 1.0, 8.0, -2.0)
    );
    assert_eq!(
        (
            fountain.before,
            fountain.drunk,
            fountain.wasted,
            fountain.after,
            fountain.net
        ),
        (8.0, 2.0, 3.0, 10.0, 2.0)
    );
    assert_eq!(fountain.win, None);
    assert_eq!(
        (l.end, l.min_body, l.worst_body, l.wasted),
        (100.0, 8.0, 8, 3.0)
    );
    assert!(l.stops.is_empty());
}

#[test]
fn level_stops_at_the_lost_fight() {
    // Two Lava units: one strike, then the slime waits on its last unit and dies to the bite.
    let mut d = level_design(
        r#"{ kind = "fight", enemy = "Mushroom", enemies = 3 },
           { kind = "fountain", liquid = "Water", units = 1 }"#,
    );
    d.levels[0].start = Some(vec![Liquid::Lava; 2]);
    let l = simulator::level::run(&d, &d.levels[0]);
    assert_eq!(l.end, 0.0);
    assert_eq!(l.rows[0].win, Some(0.0));
    assert_eq!((l.rows[1].reached, l.rows[1].win), (0.0, None));
    assert_eq!(
        l.stops.keys().collect::<Vec<_>>(),
        ["stage1:melee:Mushroom"]
    );
}

#[test]
fn level_ledger_adds_up() {
    // The shipped levels, plus Blood vs Bat: Regeneration grows units, and Blood cannot kill a
    // Vampirism bat, so some fights end in a timeout with units left.
    let mut blood = level_design(r#"{ kind = "fight", enemy = "Bat", enemies = 1 }"#);
    blood.levels[0].start = Some(vec![Liquid::Blood; 10]);
    let l = simulator::level::run(&blood, &blood.levels[0]);
    assert!(l.rows[0].grown > 0.0, "{:?}", l.rows[0]);
    let d = design();
    let runs = d
        .levels
        .iter()
        .map(|level| (level.id.clone(), simulator::level::run(&d, level)))
        .chain([("blood".to_string(), l)]);
    for (id, run) in runs {
        for r in &run.rows {
            let expected = r.before - r.spent - r.bitten - r.status + r.grown + r.eaten + r.drunk;
            assert!(
                (r.after - expected).abs() < 0.05,
                "{id} stage {}: {r:?}",
                r.stage
            );
        }
    }
}

#[test]
fn level_design_errors_are_explicit() {
    let base =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let kite = base.replacen(
        "policy = \"newcomer\"\nstages",
        "policy = \"kite\"\nstages",
        1,
    );
    assert!(Design::parse(&kite).unwrap_err().contains("not supported"));
    let long = base.replacen(
        "id = \"sample-ap1\"\ncapacity = 10",
        &format!(
            "id = \"sample-ap1\"\ncapacity = 10\nstart = [{}]",
            ["\"Lava\""; 11].join(", ")
        ),
        1,
    );
    assert!(Design::parse(&long).unwrap_err().contains("1..=capacity"));
    let dup = base.replacen("id = \"sample-ap2\"", "id = \"sample-ap1\"", 1);
    assert!(Design::parse(&dup).unwrap_err().contains("duplicate id"));
    let zero = base.replacen("action_points = 1\npolicy", "action_points = 0\npolicy", 1);
    assert!(Design::parse(&zero).unwrap_err().contains("at least 1"));
    let typo = base.replacen(
        "capacity = 10\naction_points",
        "capacity = 10\nstart_body = []\naction_points",
        1,
    );
    assert!(Design::parse(&typo).is_err());
    let ghost = base.replacen(
        "enemy = \"Bat\", enemies = 1",
        "enemy = \"Ghost\", enemies = 1",
        1,
    );
    assert!(
        Design::parse(&ghost)
            .unwrap_err()
            .contains("no enemy named Ghost")
    );
}

// The hand-checked 10 Lava vs 1 Mushroom fight, by source. Strike 1: 5->4. Its pulse: strike
// Burning 4->3. The bite pops a Lava unit; Retaliation adds a second Burning. Strike 2 comes
// while the queued Burnings (2 + 3 pulses left, 5 HP) already outweigh its 3 HP: a doomed
// strike, 3->2. Next pulse: the strike Burning 2->1, the retaliation Burning 1->0.
#[test]
fn slices_split_the_reference_fight_by_source() {
    let d = design();
    let r = report::play(&d, &pure(&d, Liquid::Lava), &one("Mushroom"), 1, 0);
    let t = &r.tally;
    assert_eq!(t.spent_by.get(&Liquid::Lava), Some(&2));
    assert_eq!(t.bitten_by.get(&Liquid::Lava), Some(&1));
    assert!(t.status_lost_by.is_empty());
    assert_eq!((t.strike_dmg, t.status_dmg, t.retaliation_dmg), (2, 2, 1));
    assert_eq!(t.status_dmg_by.get("Burning"), Some(&3));
    assert_eq!((t.overkill, t.doomed_strikes), (0, 1));

    let toml =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let rep = report::run(
        &d,
        1,
        vec![pure(&d, Liquid::Lava)],
        vec![one("Mushroom")],
        "t",
        "c",
    );
    let text = text::render(&toml, None, &[rep], &[], &[]).unwrap();
    let line = |section: &str| {
        text.split(&format!("[{section}]\n"))
            .nth(1)
            .unwrap()
            .lines()
            .nth(1)
            .unwrap()
            .split_whitespace()
            .skip(4)
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(
        line("economy"),
        "2.00 1.00 0.00 0.00 0.00 -3.00 spent=Lava:2.00 bitten=Lava:1.00 status=-"
    );
    assert_eq!(line("damage"), "2.00 2.00 1.00 0.00 1.00 2.50 Burning:3.00");
}

#[test]
fn overkill_counts_damage_beyond_the_hp_left() {
    // Two Oil strikes make the Mushroom doubly Flammable: the next Lava strike means 4 HP.
    let d = design();
    let mut f = Fight::new(
        &d,
        0,
        1,
        vec![Liquid::Oil, Liquid::Oil, Liquid::Lava, Liquid::Water],
        (1, 5),
        &[("Mushroom", (2, 5))],
    );
    f.slime_strike(0, Liquid::Oil);
    f.slime_strike(0, Liquid::Oil);
    f.slime_strike(0, Liquid::Lava);
    assert_eq!(f.enemies[0].hit_points, 0);
    let r = f.run();
    assert_eq!((r.tally.strike_dmg, r.tally.overkill), (5, 1));
}

#[test]
fn overkill_counts_a_status_pulse_beyond_the_hp_left() {
    // Lava then Oil: Burning on a Flammable Mushroom, 3 HP left. At 1 HP its Burning pulse means
    // 2 (×2 Flammable) and takes 1: overkill 1. The slime keeps one Water and only waits.
    let d = design();
    let mut f = Fight::new(
        &d,
        0,
        1,
        vec![Liquid::Lava, Liquid::Oil, Liquid::Water],
        (1, 5),
        &[("Mushroom", (2, 5))],
    );
    f.slime_strike(0, Liquid::Lava);
    f.slime_strike(0, Liquid::Oil);
    f.enemies[0].hit_points = 1;
    let r = f.run();
    assert!(r.won);
    assert_eq!((r.tally.status_dmg, r.tally.overkill), (1, 1));
}

// The balance session's case: 10 Lava vs a Mushroom whose corpse is 4 Water. The fight leaves
// 7 (2 spent, 1 bitten); 3 fit, 1 is wasted; net counts the whole corpse: 4 − 3 = +1.
#[test]
fn level_net_counts_the_whole_corpse() {
    let mut d = level_design(r#"{ kind = "fight", enemy = "Mushroom", enemies = 1 }"#);
    d.enemies
        .iter_mut()
        .find(|e| e.name == "Mushroom")
        .unwrap()
        .volume = vec![Liquid::Water; 4];
    let r = &simulator::level::run(&d, &d.levels[0]).rows[0];
    assert_eq!(
        (r.corpse, r.eaten, r.wasted, r.after, r.net),
        (4.0, 3.0, 1.0, 10.0, 1.0)
    );
}

// S13: the body is ordered edge → centre; strikes, bites and status losses take the edge unit,
// new units land on the edge. The starting mix (Water, Water, Oil, Poison, Acid, Lava) has Lava
// on the edge: the newcomer strikes Lava first, the bite then pops Acid, the next strike is
// Poison.
#[test]
fn strikes_spend_the_edge_unit() {
    let d = design();
    let start = report::builds(&d)
        .into_iter()
        .find(|b| b.id == "start")
        .unwrap();
    let (_, log) = report::fight(&d, &start, &one("Mushroom"), 1, 0).run_traced();
    let strikes: Vec<&str> = log
        .iter()
        .filter_map(|l| l.strip_prefix("Slime strikes Mushroom#1 with "))
        .map(|l| l.split(':').next().unwrap())
        .collect();
    assert_eq!(strikes[..2], ["Lava", "Poison"], "{log:#?}");
}

#[test]
fn swap_exchanges_neighbours_counted_from_the_edge() {
    let d = design();
    let mut f = Fight::new(
        &d,
        0,
        1,
        vec![Liquid::Water, Liquid::Oil, Liquid::Lava],
        (1, 5),
        &[("Mushroom", (2, 5))],
    );
    f.slime_swap(0); // edge Lava <-> Oil
    assert_eq!(f.slime.volume, [Liquid::Water, Liquid::Lava, Liquid::Oil]);
    f.slime_swap(1); // Lava <-> Water
    assert_eq!(f.slime.volume, [Liquid::Lava, Liquid::Water, Liquid::Oil]);
    f.slime_swap(2); // no unit at position 3: nothing happens
    assert_eq!(f.slime.volume, [Liquid::Lava, Liquid::Water, Liquid::Oil]);
    let r = f.run();
    assert_eq!(r.tally.swaps, 2);
}
