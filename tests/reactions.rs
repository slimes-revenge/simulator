//! Stage 3b, R1 (balance `sim-reactions-spec.md`): reactions in the body, stones,
//! encouragement.

use simulator::data::{Design, Liquid};
use simulator::fight::{Fight, FightResult, Policy};

/// One round: the kite waits (no enemy near), then the body reacts. (result, body, log)
fn round(d: &Design, body: Vec<Liquid>) -> (FightResult, Vec<Liquid>, Vec<String>) {
    let make = || {
        let mut f = Fight::new(d, 0, 1, body.clone(), (0, 5), &[("Mushroom", (9, 9))]);
        f.policy = Policy::Kite;
        f.max_rounds = 1;
        f
    };
    let (_, log) = make().run_traced();
    let (r, out) = make().run_with_volume();
    (r, out, log)
}

#[test]
fn reactions_chain_from_the_edge_and_close_the_body() {
    use Liquid::*;
    // Alkali + Acid (at the edge) -> Water; then Lava + Water -> nothing, with a stone.
    let (r, body, log) = round(&Design::bundled(), vec![Lava, Water, Alkali, Acid]);
    assert_eq!(body, vec![Water]);
    let t = &r.tally;
    assert_eq!((t.reactions, t.react_loss, t.stones), (2, 3, 1));
    assert!(
        log.iter().any(|l| l.starts_with("A stone falls")),
        "{log:?}"
    );
}

#[test]
fn poison_and_blood_make_pus() {
    use Liquid::*;
    let (r, body, _) = round(&Design::bundled(), vec![Water, Blood, Poison]);
    assert_eq!(body, vec![Water, Pus, Pus]);
    assert_eq!((r.tally.reactions, r.tally.react_loss), (1, 0));
}

#[test]
fn a_reaction_can_kill_unless_the_rule_is_off() {
    use Liquid::*;
    let (r, body, _) = round(&Design::bundled(), vec![Water, Lava]);
    assert!(body.is_empty() && !r.won);
    assert_eq!(r.cause.as_deref(), Some("reaction"));
    let mut safe = Design::bundled();
    safe.rules.reaction_can_kill = false;
    let (_, body, _) = round(&safe, vec![Water, Lava]);
    assert_eq!(body, vec![Water, Lava]);
}

#[test]
fn an_encouraged_edge_strikes_one_harder_once_per_turn() {
    use Liquid::*;
    // Water encourages Phosphorus, Aquavita encourages Water (circle I); Lava does not
    // encourage Phosphorus. The second encouraged strike of the turn gets no bonus.
    let strike = |body: Vec<Liquid>| {
        // An Alkali Mushroom: no element relation with circle I, so only encouragement counts.
        let mut d = Design::bundled();
        d.enemies
            .iter_mut()
            .find(|e| e.name == "Mushroom")
            .unwrap()
            .element = Some(Alkali);
        let mut f = Fight::new(&d, 0, 2, body, (0, 5), &[("Mushroom", (1, 5))]);
        f.policy = Policy::Newcomer;
        f.max_rounds = 1;
        f.run_traced().1
    };
    let log = strike(
        vec![Water; 7]
            .into_iter()
            .chain([Aquavita, Water, Phosphorus])
            .collect(),
    );
    let hits: Vec<&String> = log
        .iter()
        .filter(|l| l.starts_with("Slime strikes"))
        .collect();
    assert!(hits[0].contains("HP 5->3"), "{hits:?}");
    assert!(hits[1].contains("HP 3->2"), "once per turn: {hits:?}");
    let log = strike(vec![Lava, Phosphorus]);
    let hit = log.iter().find(|l| l.starts_with("Slime strikes")).unwrap();
    assert!(hit.contains("HP 5->4"), "{hit}");
}

#[test]
fn units_that_meet_at_the_exit_react_before_leaving() {
    // The river's Lava lands on the Water at the exit as the run ends: they react there.
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += "[[levels]]\nid = \"x\"\ncapacity = 3\nstart = [\"Oil\", \"Water\"]\naction_points = 1\n\
             policy = \"strafe\"\nmap = { rooms = 2, room_side = [5, 5], enemies_per_room = [0, 0], \
             enemy_kinds = [\"Mushroom\"], corridor = 1, vision = 2, river = 1, \
             river_liquids = [\"Lava\"] }\n";
    let d = Design::parse(&text).unwrap();
    let level = &d.levels[0];
    let ld = simulator::map::level_design(&d, level);
    let (r, state) = simulator::map::play(
        &ld,
        level,
        level.map.as_ref().unwrap(),
        &simulator::map::start_body(level),
        0,
    );
    assert!(r.won);
    assert_eq!(state.body, vec![Liquid::Oil]);
    assert_eq!(r.tally.react_loss, 2);
}

#[test]
fn reaction_data_is_validated() {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let three = text.replacen(
        "makes = [\"Pus\", \"Pus\"]",
        "makes = [\"Pus\", \"Pus\", \"Pus\"]",
        1,
    );
    assert!(Design::parse(&three).unwrap_err().contains("at most 2"));
    let stone = text.replacen("stone_rounds = 4", "stone_rounds = 0", 1);
    assert!(Design::parse(&stone).unwrap_err().contains("stone_rounds"));
}

#[test]
fn resin_burns_off_lava_leaving_amber_and_the_chain_goes_on() {
    use Liquid::*;
    // Resin + Lava (edge) -> Lava, amber; then Water + Lava -> nothing, a stone.
    let (r, body, _) = round(&Design::bundled(), vec![Oil, Water, Lava, Resin]);
    assert_eq!(body, vec![Oil]);
    let t = &r.tally;
    assert_eq!((t.reactions, t.react_loss, t.stones), (2, 3, 2));
}
