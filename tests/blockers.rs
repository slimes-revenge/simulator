//! A mobile blocker the slime cannot hurt (balance's ladder timeouts): the slime digs down to
//! a unit that can, or slips past it when neither side can hurt the other.

use simulator::data::Design;
use simulator::map;

/// Two small rooms joined by a 1-wide corridor, a Water Undine in each; a `kite` slime.
fn play(start: &str, seed: u64) -> (simulator::fight::FightResult, Vec<String>) {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += &format!(
        "[[levels]]\nid = \"x\"\ncapacity = 10\nstart = {start}\naction_points = 2\npolicy = \"kite\"\n\
         map = {{ rooms = 2, room_side = [4, 4], enemies_per_room = [1, 1], enemy_kinds = [\"Undine\"], \
         corridor = 1, vision = 3 }}\n"
    );
    let d = Design::parse(&text).unwrap();
    let l = &d.levels[0];
    map::fight(&d, l, l.map.as_ref().unwrap(), &map::start_body(l), seed).run_traced()
}

#[test]
fn a_slime_digs_past_a_blocker_its_edge_cannot_hurt() {
    // Seed 9: an Undine stands in the corridor; four Water units (its own element) cover the
    // Blood, more than a bring-up reaches. The slime spits Water away until Blood is near.
    let blood = "[\"Blood\", \"Blood\", \"Blood\", \"Blood\", \"Blood\", \"Blood\", \
                 \"Water\", \"Water\", \"Water\", \"Water\"]";
    let (r, log) = play(blood, 9);
    assert!(r.won, "{:?}", r.cause);
    assert!(
        log.iter()
            .any(|l| l.starts_with("Slime spits Water away to dig past Undine"))
    );
}

#[test]
fn a_slime_slips_past_a_blocker_when_neither_can_hurt_the_other() {
    // Water slime, Water Undine: same-element creatures do not fight; they swap cells.
    let (r, log) = play(
        "[\"Water\", \"Water\", \"Water\", \"Water\", \"Water\", \"Water\"]",
        9,
    );
    assert!(r.won, "{:?}", r.cause);
    assert!(
        log.iter().any(|l| l.starts_with("Slime slips past Undine")),
        "{log:?}"
    );
}

#[test]
fn it_digs_when_a_bring_up_would_react_and_never_for_pus() {
    // Phosphorus under the Water edge can hurt the Undine, but bringing it up would put Water
    // next to Lava: the slime spits the Water away instead of giving up.
    let (r, log) = play("[\"Lava\", \"Phosphorus\", \"Water\"]", 9);
    assert_ne!(r.cause.as_deref(), Some("stalemate"), "{log:?}");
    assert!(
        log.iter()
            .any(|l| l.starts_with("Slime spits Water away to dig past Undine"))
    );
    // Pus (power 0) hurts nothing: no digging toward it; the slime slips past instead.
    let (r, log) = play(
        "[\"Pus\", \"Water\", \"Water\", \"Water\", \"Water\", \"Water\"]",
        9,
    );
    assert!(r.won, "{:?}", r.cause);
    assert!(!log.iter().any(|l| l.contains("dig past")));
    assert!(log.iter().any(|l| l.starts_with("Slime slips past Undine")));
}
