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
        let d = Design::bundled();
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
