//! Stage 3d part 3 (balance `sim-3d-spec.md` D8, `builds.md`): the `b-<liquid>` and `b-auto`
//! policies, the Poisoned Living corpse.

use Liquid::*;
use simulator::data::{Design, Liquid};
use simulator::fight::{Fight, Policy};
use simulator::map;

/// `rounds` rounds of `policy` at (0, 5) next to a Mushroom of `element`: the log.
fn play(element: Liquid, body: Vec<Liquid>, policy: Policy, rounds: u32) -> Vec<String> {
    let mut d = Design::bundled();
    d.enemies
        .iter_mut()
        .find(|e| e.name == "Mushroom")
        .unwrap()
        .element = Some(element);
    let mut f = Fight::new(&d, 0, 1, body, (0, 5), &[("Mushroom", (1, 5))]);
    f.policy = policy;
    f.max_rounds = rounds;
    f.run_traced().1
}

/// 7 Lava of 10; after round 1's bite the edge is Fat with a Lava unit under it.
fn lava_body() -> Vec<Liquid> {
    let mut b = vec![Lava; 6];
    b.extend([Fat, Lava, Fat, Fat]);
    b
}

#[test]
fn build_policies_are_named_by_their_liquid() {
    let d = Design::bundled();
    assert_eq!(Policy::from_name("b-lava", &d), Policy::Build(Some(Lava)));
    assert_eq!(Policy::from_name("b-auto", &d), Policy::Build(None));
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let with = |policy: &str| {
        Design::parse(&text.replacen(
            "policy = \"lure\"\nmap",
            &format!("policy = \"{policy}\"\nmap"),
            1,
        ))
    };
    assert!(with("b-lava").is_ok());
    assert!(with("b-auto").is_ok());
    assert!(with("b-dross").is_err());
    assert!(with("b-oil").is_err());
    // The matrix and the ladder thresholds take them too.
    let matrix = text.replacen(
        "policies = [\"newcomer\", \"arranger\", \"kite\", \"reader\"]",
        "policies = [\"newcomer\", \"b-lava\", \"b-auto\"]",
        1,
    );
    assert!(Design::parse(&matrix).is_ok());
    let threshold = text.replacen(
        "policy = \"kite\", min = 70",
        "policy = \"b-acid\", min = 70",
        1,
    );
    assert_ne!(threshold, text);
    assert!(Design::parse(&threshold).is_ok());
}

#[test]
fn a_build_brings_its_liquid_up_against_a_target_it_dominates() {
    let brings = |element: Liquid, body: Vec<Liquid>, policy: Policy| {
        play(element, body, policy, 3)
            .iter()
            .any(|l| l == "Slime brings Lava to the edge")
    };
    // Lava ⊣ Aquavita.
    assert!(brings(Aquavita, lava_body(), Policy::Build(Some(Lava))));
    assert!(
        brings(Aquavita, lava_body(), Policy::Build(None)),
        "b-auto plays its affinity"
    );
    // Not against an Alkali Mushroom (no relation), not by `kite`, not without the affinity.
    assert!(!brings(Alkali, lava_body(), Policy::Build(Some(Lava))));
    assert!(!brings(Aquavita, lava_body(), Policy::Kite));
    let mut weak = lava_body();
    weak[..3].fill(Fat);
    assert!(!brings(Aquavita, weak, Policy::Build(Some(Lava))));
}

/// A map level: the slime starts with `start`; `enemies` Mushrooms a room; the river offers Fat.
fn level(policy: &str, start: &str, enemies: u32) -> Design {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += &format!(
        "[[levels]]\nid = \"x\"\ncapacity = 10\nstart = {start}\naction_points = 1\npolicy = \"{policy}\"\n\
         map = {{ rooms = 2, room_side = [5, 5], enemies_per_room = [{enemies}, {enemies}], \
         enemy_kinds = [\"Mushroom\"], corridor = 1, vision = 2, river = 6, river_liquids = [\"Fat\"] }}\n"
    );
    Design::parse(&text).unwrap()
}

fn body_out(d: &Design, seed: u64) -> Vec<Liquid> {
    let l = &d.levels[0];
    let (r, state) = map::play(d, l, l.map.as_ref().unwrap(), &map::start_body(l), seed);
    assert!(r.won, "seed {seed}");
    state.body
}

#[test]
fn a_build_drinks_only_while_its_share_holds() {
    let three = "[\"Lava\", \"Lava\", \"Lava\"]";
    // 3 Lava / 0.6 = 5 units.
    assert_eq!(body_out(&level("b-lava", three, 0), 0).len(), 5);
    assert_eq!(body_out(&level("b-auto", three, 0), 0).len(), 5);
    assert_eq!(body_out(&level("kite", three, 0), 0).len(), 9);
    // No affinity, no limit: 2 Lava of 4 is under 60 %.
    let half = "[\"Fat\", \"Fat\", \"Lava\", \"Lava\"]";
    assert_eq!(body_out(&level("b-lava", half, 0), 0).len(), 10);
}

#[test]
fn a_living_enemy_killed_while_poisoned_leaves_pus() {
    // Alkali (Living) Mushrooms of 1 Water: a Poison body leaves them Poisoned when they die.
    let poison = "[\"Poison\", \"Poison\", \"Poison\", \"Poison\", \"Poison\", \"Poison\"]";
    let d = level("kite", poison, 1);
    let won = |d: &Design, s: u64| {
        let l = &d.levels[0];
        let (r, state) = map::play(d, l, l.map.as_ref().unwrap(), &map::start_body(l), s);
        (r.won, state.body)
    };
    let pus = (0..10).any(|s| won(&d, s).1.contains(&Pus));
    assert!(pus, "a Poisoned corpse is Pus");
    let water = "[\"Water\", \"Water\", \"Water\", \"Water\", \"Water\", \"Water\"]";
    let d = level("kite", water, 1);
    assert!((0..10).all(|s| !won(&d, s).1.contains(&Pus)));
}

#[test]
fn a_killing_last_poisoned_pulse_still_leaves_pus() {
    // Poisoned lasts 1 pulse (also from Retaliation); armor 2 stops the Poison strike (2 with
    // R2's +1), so the Mushroom (1 HP) dies of the pulse that also ends its Poisoned status.
    let poison = "[\"Poison\", \"Poison\", \"Poison\", \"Poison\", \"Poison\", \"Poison\"]";
    let mut d = level("kite", poison, 1);
    (d.statuses.poisoned, d.rules.retaliation_pulses) = (1, 1);
    // No suppressed misses: a miss would kill the Mushroom without Poison.
    d.elements.suppressed_hit_percent = 100;
    let m = d.enemies.iter_mut().find(|e| e.name == "Mushroom").unwrap();
    (m.hit_points, m.armor) = (1, 2);
    let l = &d.levels[0];
    // Every corpse is Pus, so no Water (the Mushrooms' liquid) ever enters the body.
    let bodies: Vec<Vec<Liquid>> = (0..10)
        .map(|s| {
            map::play(&d, l, l.map.as_ref().unwrap(), &map::start_body(l), s)
                .1
                .body
        })
        .collect();
    assert!(bodies.iter().any(|b| b.contains(&Pus)));
    assert!(bodies.iter().all(|b| !b.contains(&Water)), "{bodies:?}");
}

#[test]
fn a_build_eats_its_liquid_first_and_keeps_its_share() {
    // Mushrooms of 3 Fat and 1 Lava; a 3-Lava `b-lava` slime eats the Lava first and leaves
    // the Fat that would take it under 60 %. `b-auto` eats everything.
    let three = "[\"Lava\", \"Lava\", \"Lava\"]";
    let play = |policy: &str| {
        let mut d = level(policy, three, 1);
        let m = d.enemies.iter_mut().find(|e| e.name == "Mushroom").unwrap();
        (m.volume, m.hit_points) = (vec![Fat, Fat, Fat, Lava], 2);
        let l = &d.levels[0];
        let mut wasted = 0;
        let mut kept = true;
        for s in 0..20 {
            let (r, state) = map::play(&d, l, l.map.as_ref().unwrap(), &map::start_body(l), s);
            wasted += state.episodes.iter().map(|e| e.wasted).sum::<usize>();
            let n = state.body.iter().filter(|&&u| u == Lava).count();
            kept &= !r.won || n * 100 >= state.body.len() * 60;
        }
        (wasted, kept)
    };
    let (wasted, kept) = play("b-lava");
    assert!(wasted > 0 && kept);
    assert_eq!(play("b-auto").0, 0, "b-auto eats every corpse");
}
