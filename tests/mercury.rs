//! Stage 3d part 2 (balance `sim-3d-spec.md` D7): the Mercury affinity as a defence, the
//! `mercury` policy.

use Liquid::*;
use simulator::data::{Design, Liquid};
use simulator::fight::{Fight, Policy};
use simulator::{ladder, map};

/// The bundled design with a Mushroom of `element` biting for `bite`.
fn design(element: Liquid, bite: i32) -> Design {
    let mut d = Design::bundled();
    let m = d.enemies.iter_mut().find(|e| e.name == "Mushroom").unwrap();
    m.element = Some(element);
    m.melee_power = bite;
    d
}

/// `rounds` rounds of `policy` at (0, 5) against enemies: (log, body after).
fn play(
    d: &Design,
    body: Vec<Liquid>,
    policy: Policy,
    enemies: &[(&str, (i32, i32))],
    rounds: u32,
) -> (Vec<String>, Vec<Liquid>, simulator::fight::FightResult) {
    let make = || {
        let mut f = Fight::new(d, 0, 1, body.clone(), (0, 5), enemies);
        f.policy = policy;
        f.max_rounds = rounds;
        f
    };
    let (_, log) = make().run_traced();
    let (r, out) = make().run_with_volume();
    (log, out, r)
}

#[test]
fn a_mercury_affinity_halves_a_bite_and_loses_non_mercury_first() {
    // The newcomer spends its Mercury edge; the bite of 3 then takes 2: Water, then Fat.
    let d = design(Alkali, 3);
    let mut body = vec![Fat];
    body.extend([Mercury; 7]);
    body.extend([Water, Mercury]);
    let mushroom = [("Mushroom", (1, 5))];
    let (log, out, r) = play(&d, body.clone(), Policy::Newcomer, &mushroom, 1);
    assert_eq!(out, vec![Mercury; 7], "{log:?}");
    assert_eq!(r.units_bitten, 2);
    assert_eq!(r.tally.bitten_by.get(&Water), Some(&1));
    assert_eq!(r.tally.bitten_by.get(&Fat), Some(&1));
    // The fallback: the full bite pops the edge.
    let mut inv = d.clone();
    inv.rules.mercury_affinity = "invisible".into();
    let (_, out, r) = play(&inv, body, Policy::Newcomer, &mushroom, 1);
    assert_eq!(r.units_bitten, 3);
    assert_eq!(out.len(), 6);
    // Without the affinity a bite is whole.
    let (_, _, r) = play(&d, vec![Water; 10], Policy::Newcomer, &mushroom, 1);
    assert_eq!(r.units_bitten, 3);
}

#[test]
fn only_the_invisible_fallback_hides_a_mercury_slime() {
    // A calm Aggressive Mushroom 3 cells away and a kiting Mercury slime.
    let d = design(Alkali, 1);
    let far = [("Mushroom", (3, 5))];
    let comes = |d: &Design| {
        let (log, _, _) = play(d, vec![Mercury; 10], Policy::Kite, &far, 1);
        log.iter().any(|l| l.starts_with("Mushroom#1 Chase"))
    };
    assert!(comes(&d), "defence: it is seen");
    let mut inv = d.clone();
    inv.rules.mercury_affinity = "invisible".into();
    assert!(!comes(&inv), "invisible: it is not");
}

#[test]
fn the_mercury_policy_goes_for_enemies_only_with_its_affinity() {
    // A Passive Bat that never comes: `mercury` walks to it with the affinity, waits without.
    let d = Design::bundled();
    let bat = [("Bat", (5, 5))];
    let (log, _, _) = play(&d, vec![Mercury; 10], Policy::Mercury, &bat, 1);
    assert!(
        log.iter().any(|l| l.starts_with("Slime goes for Bat#1")),
        "{log:?}"
    );
    let (log, _, _) = play(&d, vec![Water; 10], Policy::Mercury, &bat, 1);
    assert!(
        !log.iter().any(|l| l.starts_with("Slime goes for")),
        "{log:?}"
    );
}

#[test]
fn the_mercury_policy_strikes_with_mercury_while_the_affinity_holds() {
    let d = design(Alkali, 1);
    let mushroom = [("Mushroom", (1, 5))];
    let strike = |body: Vec<Liquid>| {
        let (log, _, _) = play(&d, body, Policy::Mercury, &mushroom, 2);
        log.into_iter()
            .filter(|l| l.starts_with("Slime brings") || l.starts_with("Slime strikes"))
            .collect::<Vec<_>>()
    };
    // 9 Mercury of 10: spending one keeps 8 of 9: bring it up, then strike with it.
    let mut body = vec![Mercury; 9];
    body.push(Water);
    let s = strike(body);
    assert_eq!(s[0], "Slime brings Mercury to the edge", "{s:?}");
    assert!(
        s[1].starts_with("Slime strikes Mushroom#1 with Mercury"),
        "{s:?}"
    );
    // 6 of 10: spending one leaves 5 of 9, under 60 %: strike with the Water edge instead.
    let mut body = vec![Fat; 3];
    body.extend([Mercury; 6]);
    body.push(Water);
    let s = strike(body);
    assert!(
        s[0].starts_with("Slime strikes Mushroom#1 with Water"),
        "{s:?}"
    );
    // A Mercury enemy: Mercury cannot hurt it, so a non-Mercury unit comes up and strikes (at
    // 2 actions: at 1 the bite takes the brought-up unit first, as the defence says).
    let d = design(Mercury, 1);
    let mut body = vec![Mercury; 7];
    body.extend([Water, Mercury, Mercury]);
    let mut f = Fight::new(&d, 0, 2, body, (0, 5), &mushroom);
    f.policy = Policy::Mercury;
    f.max_rounds = 1;
    let log = f.run_traced().1;
    assert!(
        log.iter()
            .any(|l| l.starts_with("Slime strikes Mushroom#1 with Water")),
        "{log:?}"
    );
}

/// A map level with no enemies: the slime starts with `start`; the exit's river offers `river`.
fn exit_design(policy: &str, start: &str, river: &str) -> Design {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += &format!(
        "[[levels]]\nid = \"x\"\ncapacity = 10\nstart = {start}\naction_points = 1\npolicy = \"{policy}\"\n\
         map = {{ rooms = 2, room_side = [5, 5], enemies_per_room = [0, 0], enemy_kinds = [\"Mushroom\"], \
         corridor = 1, vision = 2, river = 6, river_liquids = {river} }}\n"
    );
    Design::parse(&text).unwrap()
}

#[test]
fn the_mercury_policy_drinks_only_up_to_mercury_over_the_affinity_share() {
    let body_out = |policy: &str, river: &str| {
        let d = exit_design(policy, "[\"Mercury\", \"Mercury\", \"Mercury\"]", river);
        let level = &d.levels[0];
        let (r, state) = map::play(
            &d,
            level,
            level.map.as_ref().unwrap(),
            &map::start_body(level),
            0,
        );
        assert!(r.won);
        state.body
    };
    // 3 Mercury / 0.6 = 5 units: 2 Water, then it leaves the river.
    assert_eq!(body_out("mercury", "[\"Water\"]").len(), 5);
    assert_eq!(body_out("kite", "[\"Water\"]").len(), 9);
    // Mercury itself is always welcome.
    assert_eq!(body_out("mercury", "[\"Mercury\"]"), vec![Mercury; 9]);
}

#[test]
fn a_stop_refills_a_mercury_body_only_up_to_its_share() {
    let d = Design::bundled();
    let stop = d.ladder.stops.iter().find(|s| s.after == 4).unwrap();
    let three = [Mercury; 3];
    assert_eq!(
        ladder::at_stop(&three, stop, 10, Some(60)),
        vec![Water, Water, Mercury, Mercury, Mercury]
    );
    assert_eq!(ladder::at_stop(&three, stop, 10, None).len(), 10);
    assert_eq!(ladder::at_stop(&[Water; 3], stop, 10, Some(60)).len(), 10);
}

#[test]
fn mercury_rules_are_validated() {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let bad = text.replacen(
        "mercury_affinity = \"defence\"",
        "mercury_affinity = \"glass\"",
        1,
    );
    assert!(
        Design::parse(&bad)
            .unwrap_err()
            .contains("mercury_affinity")
    );
    let ok = text.replacen(
        "policies = [\"newcomer\", \"arranger\", \"kite\", \"reader\"]",
        "policies = [\"newcomer\", \"mercury\"]",
        1,
    );
    assert!(Design::parse(&ok).is_ok());
}
