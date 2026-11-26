//! The `planner` policy (balance: a thoughtful player; coordinator approved): `reader`, plus
//! no strikes with a unit the target suppresses while another will do, and the level's counter
//! kept near the edge between fights (v2).

use Liquid::*;
use simulator::data::{Design, Liquid};
use simulator::fight::{Fight, Policy};
use simulator::map;

fn with_element(d: &mut Design, enemy: &str, element: Liquid) {
    d.enemies
        .iter_mut()
        .find(|e| e.name == enemy)
        .unwrap()
        .element = Some(element);
}

fn log(d: &Design, body: Vec<Liquid>, enemies: &[(&str, (i32, i32))], rounds: u32) -> Vec<String> {
    log_ap(d, body, enemies, rounds, 2)
}

fn log_ap(
    d: &Design,
    body: Vec<Liquid>,
    enemies: &[(&str, (i32, i32))],
    rounds: u32,
    ap: u32,
) -> Vec<String> {
    let mut f = Fight::new(d, 0, ap, body, (0, 5), enemies);
    f.policy = Policy::Planner;
    f.max_rounds = rounds;
    f.run_traced().1
}

#[test]
fn planner_brings_up_a_unit_the_target_does_not_suppress() {
    // Poison ⊣ Alkali: against a Poison Mushroom the Alkali edge gives way to Water. It starts
    // 2 cells away, so the slime acts before its first bite.
    let mut d = Design::bundled();
    with_element(&mut d, "Mushroom", Poison);
    let l = log(
        &d,
        vec![Fat, Fat, Water, Alkali],
        &[("Mushroom", (2, 5))],
        2,
    );
    assert!(
        l.iter()
            .any(|x| x == "Slime brings Water up: the target suppresses its edge"),
        "{l:?}"
    );
    // At 1 action every action is the round's last: no bring-up, the strike would come after
    // a bite (v2).
    let l = log_ap(
        &d,
        vec![Fat, Fat, Water, Alkali],
        &[("Mushroom", (2, 5))],
        2,
        1,
    );
    assert!(
        !l.iter()
            .any(|x| x.ends_with("the target suppresses its edge")),
        "{l:?}"
    );
}

#[test]
fn planner_strikes_a_target_that_does_not_suppress_its_edge_first() {
    // A pure Alkali body: the Poison Mushroom suppresses it, the Water Undine does not.
    let mut d = Design::bundled();
    with_element(&mut d, "Mushroom", Poison);
    let l = log(
        &d,
        vec![Alkali; 6],
        &[("Mushroom", (1, 5)), ("Undine", (1, 4))],
        2,
    );
    let first = l.iter().find(|x| x.starts_with("Slime strikes")).unwrap();
    assert!(first.starts_with("Slime strikes Undine#2"), "{l:?}");
}

/// A map level of Lava Mushrooms (Water counters Lava) for `policy`, starting with `start`.
fn level(policy: &str, start: &str, enemies: u32) -> Design {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += &format!(
        "[[levels]]\nid = \"x\"\ncapacity = 10\naction_points = 2\npolicy = \"{policy}\"\nstart = {start}\n\
         map = {{ rooms = 3, room_side = [5, 6], enemies_per_room = [{enemies}, {enemies}], \
         enemy_kinds = [\"Mushroom\"], corridor = 2, vision = 2 }}\n"
    );
    let mut d = Design::parse(&text).unwrap();
    with_element(&mut d, "Mushroom", Lava);
    d
}

#[test]
fn planner_keeps_the_level_counter_near_the_edge() {
    // Water lies 5 under the edge; calm, the planner brings it up before walking on.
    let d = level(
        "planner",
        "[\"Water\", \"Fat\", \"Fat\", \"Fat\", \"Fat\", \"Fat\"]",
        1,
    );
    let l = &d.levels[0];
    let (_, log) = map::fight(&d, l, l.map.as_ref().unwrap(), &map::start_body(l), 0).run_traced();
    let first = log.iter().find(|x| x.starts_with("Slime")).unwrap();
    assert_eq!(
        first, "Slime brings Water up: the level's counter",
        "{log:?}"
    );
}

#[test]
fn planner_brings_its_counter_up_only_between_fights() {
    // The same body with every Mushroom aggroed from the start: no counter bring-up while one
    // chases the slime.
    let d = level(
        "planner",
        "[\"Water\", \"Fat\", \"Fat\", \"Fat\", \"Fat\", \"Fat\"]",
        1,
    );
    let l = &d.levels[0];
    let mut f = map::fight(&d, l, l.map.as_ref().unwrap(), &map::start_body(l), 0);
    for e in &mut f.enemies {
        e.aggroed = true;
    }
    f.max_rounds = 1;
    let log = f.run_traced().1;
    assert!(
        !log.iter().any(|x| x.ends_with("the level's counter")),
        "{log:?}"
    );
}

#[test]
fn planner_prefers_a_dominating_unit_and_never_heals_the_living() {
    // A Poison target (Poison ⊣ Alkali, Ambrosia ⊣ Poison): of Water and Ambrosia under the
    // edge, Ambrosia comes up.
    let mut d = Design::bundled();
    with_element(&mut d, "Mushroom", Poison);
    let l = log(
        &d,
        vec![Fat, Ambrosia, Water, Alkali],
        &[("Mushroom", (2, 5))],
        2,
    );
    assert!(
        l.iter()
            .any(|x| x == "Slime brings Ambrosia up: the target suppresses its edge"),
        "{l:?}"
    );
    // A Living (Alkali) target suppresses Acid (Alkali ⊣ Acid); Ambrosia would heal it.
    let d = Design::bundled();
    let l = log(
        &d,
        vec![Water, Water, Ambrosia, Acid],
        &[("Mushroom", (2, 5))],
        2,
    );
    assert!(
        !l.iter()
            .any(|x| x == "Slime brings Ambrosia up: the target suppresses its edge"),
        "{l:?}"
    );
}
