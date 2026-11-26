//! Careful eating (balance, after R3's careful drinking): careful map policies leave a corpse
//! unit that would react with their edge; `strafe` eats it.

use simulator::data::{Design, Liquid};
use simulator::map;

/// A Lava slime and Mushrooms (2 HP) whose corpse is one Water unit.
fn run(policy: &str) -> map::MapLevelResult {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += &format!(
        "[[levels]]\nid = \"x\"\ncapacity = 10\naction_points = 2\npolicy = \"{policy}\"\n\
         start = [\"Lava\", \"Lava\", \"Lava\", \"Lava\", \"Lava\", \"Lava\"]\n\
         map = {{ rooms = 3, room_side = [5, 6], enemies_per_room = [1, 1], \
         enemy_kinds = [\"Mushroom\"], corridor = 2, vision = 3 }}\n"
    );
    let mut d = Design::parse(&text).unwrap();
    let m = d.enemies.iter_mut().find(|e| e.name == "Mushroom").unwrap();
    (m.hit_points, m.volume) = (2, vec![Liquid::Water]);
    map::run(&d, &d.levels[0])
}

#[test]
fn careful_policies_leave_a_corpse_that_would_react_with_the_edge() {
    let strafe = run("strafe");
    assert!(
        strafe.reactions > 0.0,
        "strafe eats Water onto Lava: {strafe:?}"
    );
    for policy in ["kite", "lure", "reader"] {
        let r = run(policy);
        assert_eq!(r.reactions, 0.0, "{policy}: {r:?}");
    }
}
