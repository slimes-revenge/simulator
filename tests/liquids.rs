//! Stage 3a: general liquid rules (balance `liquids.md`): affinity of the capacity,
//! one-pulse Retaliation, heavy liquids, the 18 liquids.

use simulator::data::{ALL_LIQUIDS, Design, Liquid};
use simulator::fight::{Fight, Policy};

fn fight(d: &Design, body: Vec<Liquid>, ap: u32) -> Fight<'_> {
    let mut f = Fight::new(d, 0, ap, body, (0, 5), &[("Mushroom", (9, 9))]);
    f.policy = Policy::Kite;
    f.max_rounds = 1;
    f
}

#[test]
fn affinity_is_a_share_of_the_capacity() {
    use Liquid::*;
    let d = Design::bundled();
    assert_eq!(
        (d.rules.dominance_percent, d.rules.dominance_basis.as_str()),
        (60, "capacity")
    );
    // Capacity 10: 6 Lava are 60 %, 5 Lava alone (100 % of the body) are not.
    assert_eq!(fight(&d, vec![Lava; 6], 1).slime.dominant, Some(Lava));
    assert_eq!(fight(&d, vec![Lava; 5], 1).slime.dominant, None);
    let mut game = Design::bundled();
    (game.rules.dominance_percent, game.rules.dominance_basis) = (80, "volume".into());
    assert_eq!(fight(&game, vec![Lava; 5], 1).slime.dominant, Some(Lava));
}

#[test]
fn retaliation_lasts_one_pulse() {
    // A Lava slime is bitten: the biter burns for 1 pulse, not the strike's 3.
    let d = Design::bundled();
    let mut f = Fight::new(
        &d,
        0,
        1,
        vec![Liquid::Lava; 10],
        (0, 5),
        &[("Mushroom", (1, 5))],
    );
    f.policy = Policy::Kite;
    f.max_rounds = 1;
    let log = f.run_traced().1;
    let end = log
        .iter()
        .find(|l| l.starts_with("end of round 1"))
        .unwrap();
    assert!(end.contains("(Burning, 1)"), "{end}");
}

#[test]
fn heavy_liquids_cost_action_points() {
    use Liquid::*;
    let d = Design::bundled();
    let waits = |body: Vec<Liquid>, ap: u32| {
        fight(&d, body, ap)
            .run_traced()
            .1
            .iter()
            .filter(|l| l.starts_with("Slime waits"))
            .count()
    };
    let mut lava = vec![Water; 7];
    lava.extend([Lava; 3]);
    assert_eq!(waits(vec![Water; 10], 4), 4);
    assert_eq!(waits(lava.clone(), 4), 3, "3 Lava: -1");
    let mut both = lava[..5].to_vec();
    both.extend([Mercury, Mercury, Lava, Lava, Lava]);
    assert_eq!(waits(both.clone(), 4), 2, "Lava and Mercury: -2");
    assert_eq!(waits(both, 2), 2, "never below 2");
    assert_eq!(waits(lava, 1), 1, "nor below the base");
}

#[test]
fn all_18_liquids_are_listed_and_mercury_hits_for_2() {
    let d = Design::bundled();
    assert_eq!(ALL_LIQUIDS.len(), 18);
    assert_eq!(d.liquid(Liquid::Mercury).power, 2);
    assert_eq!(d.liquid(Liquid::Aquavita).power, 1);
}
