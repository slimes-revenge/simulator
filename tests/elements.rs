//! Stage 3c (balance `sim-reactions-spec.md` R2–R3, `bestiary-numbers.md`): elements in
//! combat, status immunity, Ambrosia, the `reader` policy, careful drinking.

use simulator::data::{Design, Liquid};
use simulator::fight::{Fight, FightResult, Policy};

/// The bundled design with the Mushroom of `element`.
fn design(element: Liquid) -> Design {
    let mut d = Design::bundled();
    d.enemies
        .iter_mut()
        .find(|e| e.name == "Mushroom")
        .unwrap()
        .element = Some(element);
    d
}

/// One round next to a Mushroom: (log, result, body).
fn round(
    d: &Design,
    body: Vec<Liquid>,
    policy: Policy,
    seed: u64,
) -> (Vec<String>, FightResult, Vec<Liquid>) {
    let make = || {
        let mut f = Fight::new(d, seed, 1, body.clone(), (0, 5), &[("Mushroom", (1, 5))]);
        f.policy = policy;
        f.max_rounds = 1;
        f
    };
    let (r, log) = make().run_traced();
    let (_, out) = make().run_with_volume();
    (log, r, out)
}

fn strike(log: &[String]) -> &str {
    log.iter()
        .find(|l| l.starts_with("Slime strikes"))
        .map_or("", |l| l.as_str())
}

fn with_edge(edge: &[Liquid]) -> Vec<Liquid> {
    let mut body = vec![Liquid::Fat; 10 - edge.len()];
    body.extend(edge);
    body
}

#[test]
fn a_dominating_edge_hits_one_harder() {
    // Water ⊣ Lava.
    let (log, r, _) = round(
        &design(Liquid::Lava),
        with_edge(&[Liquid::Water]),
        Policy::Newcomer,
        0,
    );
    assert!(strike(&log).contains("HP 5->3"), "{log:?}");
    assert_eq!(r.tally.elem_plus, 1);
}

#[test]
fn a_suppressed_edge_hits_plain_and_loses_encouragement() {
    // Water ⊣ Lava: a Lava edge against a Water Mushroom, encouraged by Ichor, hits 1.
    let (log, _, _) = round(
        &design(Liquid::Water),
        with_edge(&[Liquid::Ichor, Liquid::Lava]),
        Policy::Newcomer,
        0,
    );
    let hit = strike(&log);
    assert!(hit.contains("HP 5->4"), "{hit}");
    assert!(
        !hit.contains("Burning"),
        "Water is immune to Burning: {hit}"
    );
}

#[test]
fn the_same_element_does_nothing_and_keeps_the_unit() {
    let (log, r, body) = round(
        &design(Liquid::Water),
        vec![Liquid::Water; 10],
        Policy::Newcomer,
        0,
    );
    assert!(
        log.iter().any(|l| l.contains("shrugs off its own element")),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|l| l.contains("cannot hurt its own element")),
        "{log:?}"
    );
    assert_eq!((r.units_spent, r.units_bitten, body.len()), (0, 0, 10));
}

#[test]
fn a_dominating_enemy_bites_one_harder() {
    // Mercury ⊣ Water: the bite pops 2.
    let (_, r, _) = round(
        &design(Liquid::Mercury),
        with_edge(&[Liquid::Fat, Liquid::Water]),
        Policy::Kite,
        0,
    );
    assert_eq!(r.units_bitten, 2);
}

#[test]
fn a_suppressed_enemy_hits_a_quarter_of_the_time_and_hurts_itself_on_a_miss() {
    // Water ⊣ Lava: a Lava Mushroom biting a Water edge (kite waits, it bites once).
    let d = design(Liquid::Lava);
    let (mut misses, mut hits) = (0, 0);
    for seed in 0..400 {
        let (_, r, _) = round(&d, vec![Liquid::Water; 10], Policy::Kite, seed);
        misses += r.tally.elem_miss;
        hits += r.units_bitten;
    }
    assert_eq!(misses + hits, 400);
    assert!((60..=140).contains(&hits), "about 25 % of 400: {hits}");
}

#[test]
fn statuses_skip_immune_elements_and_double_on_weak_ones() {
    // Poison on a Water Mushroom: immune. Acid on a Mercury one: two Corroding copies.
    let (log, _, _) = round(
        &design(Liquid::Water),
        with_edge(&[Liquid::Poison]),
        Policy::Newcomer,
        0,
    );
    assert!(!strike(&log).contains("Poisoned"), "{log:?}");
    let (log, _, _) = round(
        &design(Liquid::Mercury),
        with_edge(&[Liquid::Acid]),
        Policy::Newcomer,
        0,
    );
    assert_eq!(strike(&log).matches("Corroding").count(), 2, "{log:?}");
}

#[test]
fn ambrosia_heals_the_living_instead_of_hurting() {
    let (log, r, _) = round(
        &design(Liquid::Alkali),
        with_edge(&[Liquid::Ambrosia]),
        Policy::Newcomer,
        0,
    );
    assert!(strike(&log).contains("HP 5->5"), "{log:?}");
    assert_eq!(r.units_spent, 1);
}

#[test]
fn reader_brings_a_dominating_unit_to_the_edge() {
    // A Lava Mushroom; Water (⊣ Lava) lies under the Fat edge (after round 1's bite).
    let d = design(Liquid::Lava);
    let body = with_edge(&[Liquid::Water, Liquid::Fat, Liquid::Fat]);
    let mut f = Fight::new(&d, 0, 2, body.clone(), (0, 5), &[("Mushroom", (1, 5))]);
    f.policy = Policy::Reader;
    f.max_rounds = 3;
    let log = f.run_traced().1;
    assert!(
        log.iter().any(|l| l == "Slime brings Water to the edge"),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|l| l.starts_with("Slime strikes Mushroom#1 with Water")),
        "{log:?}"
    );
    let mut f = Fight::new(&d, 0, 2, body, (0, 5), &[("Mushroom", (1, 5))]);
    f.policy = Policy::Kite;
    f.max_rounds = 3;
    assert!(
        !f.run_traced()
            .1
            .iter()
            .any(|l| l.starts_with("Slime brings"))
    );
}

/// A map level: the exit's river offers `river`; the slime starts with `start`.
fn exit_design(policy: &str, start: &str, river: &str) -> Design {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += &format!(
        "[[levels]]\nid = \"x\"\ncapacity = 4\nstart = {start}\naction_points = 1\npolicy = \"{policy}\"\n\
         map = {{ rooms = 2, room_side = [5, 5], enemies_per_room = [0, 0], enemy_kinds = [\"Mushroom\"], \
         corridor = 1, vision = 2, river = 2, river_liquids = {river} }}\n"
    );
    Design::parse(&text).unwrap()
}

#[test]
fn careful_policies_pour_away_what_would_react_and_strafe_does_not() {
    use simulator::map;
    let play = |policy: &str| {
        let d = exit_design(policy, "[\"Fat\", \"Water\"]", "[\"Lava\"]");
        let level = &d.levels[0];
        let (r, state) = map::play(
            &d,
            level,
            level.map.as_ref().unwrap(),
            &map::start_body(level),
            0,
        );
        (r.won, state.body, r.tally.reactions)
    };
    // lure pours both Lava units away; strafe drinks them and they react with the Water.
    assert_eq!(play("lure"), (true, vec![Liquid::Fat, Liquid::Water], 0));
    let (won, _, reactions) = play("strafe");
    assert!(won && reactions >= 1);
}

#[test]
fn an_enemy_can_be_immune_on_its_own() {
    // A Lava Mushroom takes Instability from Alkali, unless it lists it as its own immunity.
    let mut d = design(Liquid::Lava);
    let (log, _, _) = round(&d, with_edge(&[Liquid::Alkali]), Policy::Newcomer, 0);
    assert!(strike(&log).contains("Instability"), "{log:?}");
    d.enemies
        .iter_mut()
        .find(|e| e.name == "Mushroom")
        .unwrap()
        .status_immune = vec!["Instability".into()];
    let (log, _, _) = round(&d, with_edge(&[Liquid::Alkali]), Policy::Newcomer, 0);
    assert!(!strike(&log).contains("Instability"), "{log:?}");
}

#[test]
fn no_strike_is_repeated_on_its_own_element() {
    use Liquid::*;
    // Kite: a Fat unit lies 1 under the Water edge facing a Water Mushroom: bring it up.
    let d = design(Water);
    let (log, _, _) = round(&d, with_edge(&[Fat, Water]), Policy::Kite, 0);
    assert!(!log.iter().any(|l| l.contains("shrugs off")), "{log:?}");
    // A pure Water body has nothing to bring up: the matrix fight ends as a stalemate at once,
    // the newcomer after one wasted strike.
    let water = vec![Water; 10];
    let mut f = Fight::new(&d, 0, 2, water.clone(), (0, 5), &[("Mushroom", (1, 5))]);
    f.policy = Policy::Kite;
    let r = f.run();
    assert_eq!(
        (r.won, r.cause.as_deref(), r.rounds),
        (false, Some("stalemate"), 1)
    );
    let mut f = Fight::new(&d, 0, 2, water, (0, 5), &[("Mushroom", (1, 5))]);
    f.policy = Policy::Newcomer;
    let (r, log) = f.run_traced();
    assert_eq!(r.cause.as_deref(), Some("stalemate"));
    assert_eq!(
        log.iter().filter(|l| l.contains("shrugs off")).count(),
        1,
        "{log:?}"
    );
}

#[test]
fn a_stationary_blocker_of_its_own_element_is_slipped_past() {
    // Water Cacti (speed 0) in the passages, a pure Water slime: no endless useless strikes;
    // neither can hurt the other, so the slime slips past them.
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += "[[levels]]\nid = \"b\"\ncapacity = 10\naction_points = 2\npolicy = \"strafe\"\n\
             map = { rooms = 3, room_side = [5, 5], enemies_per_room = [0, 0], \
             enemy_kinds = [\"Mushroom\"], corridor = 2, vision = 2, blockers = [\"Cactus\"], \
             blockers_per_level = 2 }\n";
    let mut d = Design::parse(&text).unwrap();
    let c = d.enemies.iter_mut().find(|e| e.name == "Cactus").unwrap();
    (c.element, c.speed) = (Some(Liquid::Water), 0);
    let r = simulator::map::run(&d, &d.levels[0]);
    assert!(
        !r.stops.keys().any(|k| k.ends_with(":timeout")),
        "{:?}",
        r.stops
    );
    assert!(
        !r.stops.keys().any(|k| k.ends_with(":stalemate")),
        "{:?}",
        r.stops
    );
    assert_eq!(r.end, 100.0, "{:?}", r.stops);
}
