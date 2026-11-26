//! 4a (balance `liquid-roles.md`): Ichor's Cold fire and Blinded, Sulfur's Flammable 2 x 2 and
//! its burning affinity.

use Liquid::*;
use simulator::data::{Design, Liquid};
use simulator::fight::{Fight, Policy};

/// One `newcomer` round against a Mushroom of `element` (5 HP): the end-of-round trace line
/// from the Mushroom on.
fn struck(element: Liquid, body: Vec<Liquid>) -> String {
    let mut d = Design::bundled();
    d.enemies
        .iter_mut()
        .find(|e| e.name == "Mushroom")
        .unwrap()
        .element = Some(element);
    let mut f = Fight::new(&d, 0, 1, body, (0, 5), &[("Mushroom", (1, 5))]);
    f.policy = Policy::Newcomer;
    f.max_rounds = 1;
    let log = f.run_traced().1;
    let end = log
        .iter()
        .find(|l| l.starts_with("end of round 1"))
        .unwrap();
    end[end.find("Mushroom#1").unwrap()..].to_string()
}

fn edge(l: Liquid) -> Vec<Liquid> {
    vec![Fat, Fat, Fat, l]
}

#[test]
fn ichor_burns_with_cold_fire_and_blinds() {
    // Strike 1, then a Cold fire pulse 1 at the Mushroom's turn: 5 -> 3. Both statuses tick
    // once (3 -> 2).
    let m = struck(Alkali, edge(Ichor));
    assert!(m.contains("HP 3/5"), "{m}");
    assert!(
        m.contains("(ColdFire, 2)") && m.contains("(Blinded, 2)"),
        "{m}"
    );
    // Cold fire is not Burning: a Lava Mushroom (immune to Burning) still takes it.
    assert!(struck(Lava, edge(Ichor)).contains("ColdFire"));
    // Water is immune to Cold fire, not to Blinded.
    let w = struck(Water, edge(Ichor));
    assert!(!w.contains("ColdFire") && w.contains("Blinded"), "{w}");
}

#[test]
fn sulfur_makes_flammable_for_two_pulses_and_its_affinity_burns() {
    let m = struck(Alkali, edge(Sulfur));
    assert!(
        m.contains("(Flammable, 1)") && !m.contains("Burning"),
        "{m}"
    );
    // A Sulfur affinity: the strike also sets the target on fire.
    let m = struck(Alkali, vec![Sulfur; 10]);
    assert!(m.contains("Burning"), "{m}");
}
