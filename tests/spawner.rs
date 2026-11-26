//! The map spawner (Oleg; game `docs/design/level-generator.md`): roaming singles released
//! every `every` slime rounds, at most `max_alive` alive at once.

use simulator::data::Design;
use simulator::map;

/// Five rooms, no placed enemies, a spawner of `kinds`; a `strafe` slime of full Water.
fn design(spawner: &str) -> Design {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += &format!(
        "[[levels]]\nid = \"x\"\ncapacity = 10\naction_points = 2\npolicy = \"strafe\"\n\
         map = {{ rooms = 5, room_side = [6, 8], enemies_per_room = [0, 0], enemy_kinds = [\"Bat\"], \
         corridor = 2, vision = 2{spawner} }}\n"
    );
    Design::parse(&text).unwrap()
}

fn releases(d: &Design, seed: u64) -> (Vec<String>, map::MapLevelResult) {
    let l = &d.levels[0];
    let (_, log) =
        map::fight(d, l, l.map.as_ref().unwrap(), &map::start_body(l), seed).run_traced();
    (log, map::run(d, l))
}

#[test]
fn a_spawner_releases_on_its_rounds_up_to_max_alive() {
    // Passive Bats never fight, so none dies: the cap alone stops the releases.
    for max in [1, 2] {
        let d = design(&format!(
            ", spawner = {{ kinds = [\"Bat\"], every = 3, max_alive = {max} }}"
        ));
        let (log, result) = releases(&d, 0);
        let out: Vec<usize> = log
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains("comes out of the spawner"))
            .map(|(k, _)| k)
            .collect();
        assert_eq!(out.len(), max, "{log:?}");
        // The first one comes after round 3's slime turn, not before.
        let round3 = log.iter().position(|l| l == "round 3").unwrap();
        let round4 = log.iter().position(|l| l == "round 4").unwrap();
        assert!(out[0] > round3 && out[0] < round4);
        // The next one waits for round 6, not round 4.
        if max == 2 {
            let round6 = log.iter().position(|l| l == "round 6").unwrap();
            let round7 = log.iter().position(|l| l == "round 7").unwrap();
            assert!(out[1] > round6 && out[1] < round7, "{log:?}");
        }
        // Over all seeds a Bat struck in a corridor may die and free a place.
        assert!(result.spawned >= max as f64);
    }
}

#[test]
fn without_a_spawner_nothing_comes() {
    let (log, result) = releases(&design(""), 0);
    assert!(!log.iter().any(|l| l.contains("spawner")));
    assert_eq!(result.spawned, 0.0);
}

#[test]
fn spawner_rules_are_validated() {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut base = text[..text.find("[[levels]]").unwrap()].to_string();
    base += "[[levels]]\nid = \"x\"\ncapacity = 10\naction_points = 2\npolicy = \"strafe\"\n";
    let err = |rooms: u32, spawner: &str| {
        Design::parse(&format!(
            "{base}map = {{ rooms = {rooms}, room_side = [6, 8], enemies_per_room = [0, 0], \
             enemy_kinds = [\"Bat\"], corridor = 2, vision = 2, spawner = {spawner} }}\n"
        ))
        .unwrap_err()
    };
    assert!(err(5, "{ kinds = [], every = 3, max_alive = 1 }").contains("map.spawner"));
    assert!(err(5, "{ kinds = [\"Bat\"], every = 0, max_alive = 1 }").contains("map.spawner"));
    assert!(err(5, "{ kinds = [\"Bat\"], every = 3, max_alive = 0 }").contains("map.spawner"));
    assert!(err(2, "{ kinds = [\"Bat\"], every = 3, max_alive = 1 }").contains("map.spawner"));
    assert!(err(5, "{ kinds = [\"Rat\"], every = 3, max_alive = 1 }").contains("Rat"));
}
