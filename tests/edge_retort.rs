//! 4c (balance `liquid-roles.md`): without an affinity, the edge unit a bite takes leaves its
//! status on the biter for one of its turns; the affinity still retaliates in full.

use Liquid::*;
use simulator::data::{Design, Liquid};
use simulator::fight::{Fight, Policy};

/// One round of a waiting `kite` slime bitten by an Acid Mushroom: the Mushroom's statuses
/// at the round's end, as the trace prints them.
fn after_bite(d: &Design, body: Vec<Liquid>) -> String {
    let mut d = d.clone();
    d.enemies
        .iter_mut()
        .find(|e| e.name == "Mushroom")
        .unwrap()
        .element = Some(Acid);
    let mut f = Fight::new(&d, 0, 1, body, (0, 5), &[("Mushroom", (1, 5))]);
    f.policy = Policy::Kite;
    f.max_rounds = 1;
    let log = f.run_traced().1;
    let end = log
        .iter()
        .find(|l| l.starts_with("end of round 1"))
        .unwrap();
    end[end.find("Mushroom#1").unwrap()..].to_string()
}

#[test]
fn a_bitten_edge_leaves_its_status_on_the_biter_for_one_turn() {
    // No affinity: Fat, Water, Fat, Water, Poison (edge). The bite takes the Poison.
    let mixed = vec![Fat, Water, Fat, Water, Poison];
    let d = Design::bundled();
    assert!(after_bite(&d, mixed.clone()).contains("(Poisoned, 2)"));
    // The fallback turns it off.
    let mut off = d.clone();
    off.rules.edge_retort_pulses = 0;
    assert!(!after_bite(&off, mixed).contains("Poisoned"));
    // An affinity retaliates in full (`retaliation_pulses`, 3).
    assert!(after_bite(&d, vec![Poison; 10]).contains("(Poisoned, 3)"));
}

#[test]
fn edge_retort_rules_are_validated() {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let bad = text.replacen("edge_retort_pulses = 2", "edge_retort_pulses = -1", 1);
    assert_ne!(bad, text);
    assert!(
        Design::parse(&bad)
            .unwrap_err()
            .contains("edge_retort_pulses")
    );
}
