//! Acceptance checks for the design guard (docs/design/design-guard.md, first version).

use simulator::data::{Design, Liquid};
use simulator::fight::{Fight, FightResult};
use simulator::report::{self, Build, Encounter};

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

fn result(won: bool, rounds: u32, left: usize, spent: usize, cause: Option<&str>) -> FightResult {
    FightResult {
        won,
        rounds,
        units_left: left,
        units_spent: spent,
        cause: cause.map(String::from),
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
    assert_eq!(r, result(true, 4, 7, 2, None));
}

// With two actions: round 1 two steps, the Mushroom steps (distance 2); round 2 step + strike
// (4 HP), Burning 4→3, bite, retaliation Burning; round 3 two strikes 3→2→1, its first
// Burning pulse kills it. 3 strikes, 1 bite: 6 left.
#[test]
fn ten_lava_vs_one_mushroom_two_actions() {
    let d = design();
    let r = report::play(&d, &pure(&d, Liquid::Lava), &one("Mushroom"), 2, 0);
    assert_eq!(r, result(true, 3, 6, 3, None));
}

// Water deals 1 per strike and nothing over time: from round 3 each round is one strike and
// one bite. Strikes at 10, 8, 6, 4, 2 units; the fifth kills it at round 7 with 1 unit left.
#[test]
fn ten_water_vs_one_mushroom_one_action() {
    let d = design();
    let r = report::play(&d, &pure(&d, Liquid::Water), &one("Mushroom"), 1, 0);
    assert_eq!(r, result(true, 7, 1, 5, None));
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
    let cell = report::run(&d, 1, "t", "c")
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
        let r = report::run(&d, ap, "t", "c");
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
        report::run(d, 1, "t", "c")
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
    let json = serde_json::to_value(report::run(&d, 2, "2026-01-01T00:00:00Z", "abc")).unwrap();
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
