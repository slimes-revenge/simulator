//! Stage 3d part 1 (balance `sim-3d-spec.md` D1, D3–D6): puddles, Pus burps, Infected, the
//! stops between maps, the `pus` policy.

use simulator::data::{Design, Liquid, Stop};
use simulator::fight::{Fight, Policy};
use simulator::{ladder, map};

/// A kite slime at (0, 5) and `enemies` with `puddles` on the floor, for `rounds` rounds: the log.
fn play(
    d: &Design,
    body: Vec<Liquid>,
    enemies: &[(&str, (i32, i32))],
    puddles: &[((i32, i32), Liquid)],
    rounds: u32,
) -> Vec<String> {
    let make = || {
        let mut f = Fight::new(d, 0, 1, body.clone(), (0, 5), enemies);
        f.policy = Policy::Kite;
        f.max_rounds = rounds;
        f.puddles = puddles.iter().map(|&(c, l)| (c, l, 1)).collect();
        f
    };
    make().run_traced().1
}

fn pus_body() -> Vec<Liquid> {
    let mut b = vec![Liquid::Water; 4];
    b.extend([Liquid::Pus; 6]);
    b
}

#[test]
fn a_bad_pus_affinity_burps_every_third_round() {
    // A Passive Bat far away: nothing happens but the burps.
    let d = Design::bundled();
    let log = play(&d, pus_body(), &[("Bat", (9, 9))], &[], 3);
    let burps: Vec<&String> = log
        .iter()
        .filter(|l| l.starts_with("Slime burps"))
        .collect();
    assert_eq!(burps.len(), 1, "{log:?}");
    assert!(burps[0].contains("Pus, 1"), "{burps:?}");
    let round3 = log.iter().position(|l| l == "round 3").unwrap();
    assert!(
        !log[round3..].iter().any(|l| l.starts_with("Slime waits")),
        "the burp takes the turn"
    );
    // 5 Pus of 10 is no bad affinity: no burp.
    let mut five = pus_body();
    five[4] = Liquid::Water;
    let log = play(&d, five, &[("Bat", (9, 9))], &[], 3);
    assert!(!log.iter().any(|l| l.starts_with("Slime burps")));
}

#[test]
fn a_pus_puddle_infects_the_living_who_step_in_and_weakens_their_bite() {
    // A Living (Alkali) Mushroom chases the slime through a Pus puddle at (2, 5).
    let d = Design::bundled();
    let water = vec![Liquid::Water; 10];
    let log = play(
        &d,
        water.clone(),
        &[("Mushroom", (3, 5))],
        &[((2, 5), Liquid::Pus)],
        3,
    );
    let end1 = log
        .iter()
        .find(|l| l.starts_with("end of round 1"))
        .unwrap();
    assert!(end1.contains("Infected"), "{end1}");
    // Its 1-point bite is gone while Infected.
    assert!(
        log.iter().any(|l| l.contains("too sick to bite")),
        "{log:?}"
    );
    // The Water Undine is not Living: no Infected (an Oil body, so the slime does not stall).
    let log = play(
        &d,
        vec![Liquid::Oil; 10],
        &[("Undine", (3, 5))],
        &[((2, 5), Liquid::Pus)],
        1,
    );
    let end1 = log
        .iter()
        .find(|l| l.starts_with("end of round 1"))
        .unwrap();
    assert!(!end1.contains("Infected"), "{end1}");
}

#[test]
fn other_puddles_give_their_liquid_effect_without_damage() {
    let d = Design::bundled();
    let log = play(
        &d,
        vec![Liquid::Water; 10],
        &[("Mushroom", (3, 5))],
        &[((2, 5), Liquid::Poison)],
        1,
    );
    let end1 = log
        .iter()
        .find(|l| l.starts_with("end of round 1"))
        .unwrap();
    assert!(
        end1.contains("Poisoned") && end1.contains("HP 5/5"),
        "{end1}"
    );
}

#[test]
fn pus_strikes_infect_only_where_the_level_says() {
    let d = Design::bundled();
    let body = vec![Liquid::Pus; 10];
    for (flag, infected) in [(false, false), (true, true)] {
        let mut f = Fight::new(&d, 0, 1, body.clone(), (0, 5), &[("Mushroom", (1, 5))]);
        f.policy = Policy::Newcomer;
        f.max_rounds = 1;
        f.pus_infects = flag;
        let log = f.run_traced().1;
        let hit = log
            .iter()
            .find(|l| l.starts_with("Slime spits Pus"))
            .unwrap();
        let end1 = log
            .iter()
            .find(|l| l.starts_with("end of round 1"))
            .unwrap();
        assert_eq!(end1.contains("Infected"), infected, "{hit} / {end1}");
    }
}

#[test]
fn a_stop_resets_the_body_and_refills_its_empty_cells_with_water() {
    use Liquid::*;
    let d = Design::bundled();
    let stop = |after: u32| -> &Stop { d.ladder.stops.iter().find(|s| s.after == after).unwrap() };
    assert_eq!(
        ladder::at_stop(&[Blood, Poison, Water], stop(4), 5),
        vec![Water, Water, Pus, Water, Water]
    );
    // At most half the body's Water turns to Poison, edge-most first; Aquavita leaves.
    assert_eq!(
        ladder::at_stop(&[Water, Water, Aquavita, Water, Water], stop(8), 5),
        vec![Water, Water, Water, Poison, Poison]
    );
    // Pus is never pushed out: a full Pus body stays full Pus.
    assert_eq!(ladder::at_stop(&[Pus; 6], stop(4), 6), vec![Pus; 6]);
}

#[test]
fn the_pus_policy_burps_and_gathers_until_full_then_leaves() {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += "[[levels]]\nid = \"p\"\ncapacity = 10\nstart = [\"Water\", \"Pus\", \"Pus\", \"Pus\", \
             \"Pus\", \"Pus\", \"Pus\"]\naction_points = 2\npolicy = \"pus\"\n\
             map = { rooms = 2, room_side = [5, 5], enemies_per_room = [0, 0], \
             enemy_kinds = [\"Mushroom\"], corridor = 1, vision = 2 }\n";
    let d = Design::parse(&text).unwrap();
    let level = &d.levels[0];
    let (r, state) = map::play(
        &d,
        level,
        level.map.as_ref().unwrap(),
        &map::start_body(level),
        0,
    );
    assert!(r.won, "{r:?}");
    assert_eq!(state.body.len(), 10);
    assert_eq!(state.body.iter().filter(|&&l| l == Liquid::Pus).count(), 9);
    assert!(r.tally.burped >= 3 && state.pus_affinity);
}

#[test]
fn the_pus_policy_never_drinks_itself_out_of_its_bad_affinity() {
    // 6 Pus of 10 at the exit; a Water river would make it 6 of 11: it leaves the river.
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += "[[levels]]\nid = \"q\"\ncapacity = 12\nstart = [\"Water\", \"Water\", \"Water\", \"Water\", \
             \"Pus\", \"Pus\", \"Pus\", \"Pus\", \"Pus\", \"Pus\"]\naction_points = 2\npolicy = \"pus\"\n\
             map = { rooms = 1, room_side = [5, 5], enemies_per_room = [0, 0], \
             enemy_kinds = [\"Mushroom\"], corridor = 1, vision = 2, river = 6, \
             river_liquids = [\"Water\"] }\n";
    let d = Design::parse(&text).unwrap();
    let level = &d.levels[0];
    let (r, state) = map::play(
        &d,
        level,
        level.map.as_ref().unwrap(),
        &map::start_body(level),
        0,
    );
    assert!(r.won && state.pus_affinity, "{r:?} {:?}", state.body);
    let water = state.body.iter().filter(|&&l| l == Liquid::Water).count();
    let pus = state.body.len() - water;
    assert!(pus * 100 >= state.body.len() * 60, "{:?}", state.body);
}

#[test]
fn before_level_9_the_pus_policy_never_spends_pus_on_the_living() {
    // A Living Mushroom per room; Pus is a bad affinity; no `pus_infects`. (Once the affinity
    // is gone the policy is `kite`, which may strike.)
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += "[[levels]]\nid = \"l\"\ncapacity = 10\nstart = [\"Water\", \"Water\", \"Pus\", \"Pus\", \
             \"Pus\", \"Pus\", \"Pus\", \"Pus\", \"Pus\", \"Pus\"]\naction_points = 2\npolicy = \"pus\"\n\
             map = { rooms = 2, room_side = [5, 6], enemies_per_room = [1, 1], \
             enemy_kinds = [\"Mushroom\"], corridor = 2, vision = 3 }\n";
    let d = Design::parse(&text).unwrap();
    let level = &d.levels[0];
    let mut stepped = false;
    for seed in 0..20 {
        let log = map::fight(
            &d,
            level,
            level.map.as_ref().unwrap(),
            &map::start_body(level),
            seed,
        )
        .run_traced()
        .1;
        // While the last round ended with Pus dominant, no strike and no Pus spit.
        let mut pus = true;
        for l in &log {
            if l.starts_with("end of round") {
                pus = l.contains("dominant Some(Pus)");
            }
            let hits = l.starts_with("Slime spits Pus") || l.starts_with("Slime strikes");
            assert!(!(pus && hits), "seed {seed}: {l} with the affinity held");
        }
        stepped |= log.iter().any(|l| l.contains("behind its puddles"));
    }
    assert!(stepped, "it steps back from the Living at least once");
}

#[test]
fn a_body_of_nothing_but_dross_is_dead() {
    let d = Design::bundled();
    let f = Fight::new(&d, 0, 1, vec![Liquid::Dross; 3], (0, 5), &[("Bat", (9, 9))]);
    assert!(!f.slime.alive());
    let f = Fight::new(
        &d,
        0,
        1,
        vec![Liquid::Dross, Liquid::Water],
        (0, 5),
        &[("Bat", (9, 9))],
    );
    assert!(f.slime.alive());
}
