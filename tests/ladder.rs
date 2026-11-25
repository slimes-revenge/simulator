//! The ladder: river, drips, sun, the run mode carrying the body, the summary.

use simulator::data::{Design, Liquid};
use simulator::{ladder, map};

/// The bundled design without its levels, plus `levels` (TOML `[[levels]]` entries).
fn design(levels: &str) -> Result<Design, String> {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    Design::parse(&(text[..text.find("[[levels]]").unwrap()].to_string() + levels))
}

/// A map level row; `map_extra` goes inside the `map` table.
fn row(id: &str, capacity: usize, start: &str, ladder: &str, map_extra: &str) -> String {
    format!(
        "[[levels]]\nid = \"{id}\"\ncapacity = {capacity}\nstart = {start}\naction_points = 2\n\
         policy = \"strafe\"\n{ladder}\nmap = {{ rooms = 3, room_side = [5, 7], \
         enemies_per_room = [0, 0], enemy_kinds = [\"Mushroom\"], corridor = 2, vision = 2{map_extra} }}\n"
    )
}

/// Plays the design's first level on `seed`: (won, end body, cause).
fn play(d: &Design, seed: u64) -> (bool, Vec<Liquid>, Option<String>) {
    let level = &d.levels[0];
    let ld = map::level_design(d, level);
    let (r, s) = map::play(
        &ld,
        level,
        level.map.as_ref().unwrap(),
        &map::start_body(level),
        seed,
    );
    (r.won, s.body, r.cause)
}

#[test]
fn the_slime_drinks_the_river_at_the_exit_in_order_until_full() {
    use Liquid::*;
    let d = design(&row(
        "r",
        7,
        "[\"Water\", \"Water\"]",
        "",
        ", river = 6, river_liquids = [\"Oil\", \"Acid\"]",
    ))
    .unwrap();
    for seed in 0..5 {
        let (won, body, _) = play(&d, seed);
        assert!(won);
        // Edge last: the river's units go onto the edge in its order; 5 fit, 1 stays.
        assert_eq!(body, vec![Water, Water, Oil, Acid, Oil, Acid, Oil]);
    }
}

#[test]
fn drips_on_the_corridors_are_drunk_on_the_way() {
    let d = design(&row(
        "d",
        10,
        "[\"Water\"]",
        "",
        ", drips = { cells = 2, units = 2 }",
    ))
    .unwrap();
    for seed in 0..5 {
        let (won, body, _) = play(&d, seed);
        assert!(won);
        assert_eq!(body, vec![Liquid::Water; 5], "seed {seed}");
    }
}

#[test]
fn the_sun_dries_the_edge_most_water_and_can_kill() {
    use Liquid::*;
    let sun = ", surface_rooms = 3, sun_every = 1";
    let d = design(&row("s", 3, "[\"Oil\", \"Water\", \"Water\"]", "", sun)).unwrap();
    let (won, body, _) = play(&d, 0);
    assert!(won && body == vec![Oil], "Oil is not Water: {body:?}");
    let d = design(&row("s", 1, "[\"Water\"]", "", sun)).unwrap();
    let (won, body, cause) = play(&d, 0);
    assert!(!won && body.is_empty());
    assert_eq!(cause.as_deref(), Some("sun"));
    // Night: every 2 rounds; the walk is longer than 4 rounds, so 2 Water units are too few.
    let night = ", surface_rooms = 3, sun_every = 2";
    let d = design(&row("s", 2, "[\"Water\", \"Water\"]", "", night)).unwrap();
    assert!(!play(&d, 0).0);
}

#[test]
fn a_run_carries_the_body_and_the_summary_finds_the_failing_level() {
    // Level 1 swaps the Water body for Oil at the exit; level 2's sun kills a Water body only.
    // Level mode: level 2 starts from Water and dies; run mode: it arrives as Oil and lives.
    let rows = row(
        "a",
        2,
        "[\"Water\"]",
        "ladder = { level = 1, difficulty = \"E\" }",
        ", river = 1, river_liquids = [\"Oil\"]",
    ) + &row(
        "b",
        2,
        "[\"Water\"]",
        "ladder = { level = 2, difficulty = \"E\", branch = \"Oil\" }",
        ", surface_rooms = 3, sun_every = 1",
    );
    let d = design(&rows).unwrap();
    let (out, summary) = ladder::run(&d);
    let end = |mode: &str, level: u32| {
        out.iter()
            .find(|r| r.mode == mode && r.place.level == level)
            .unwrap()
            .result
            .end
    };
    assert_eq!(end("level", 1), 100.0);
    assert_eq!(end("level", 2), 0.0);
    assert_eq!(end("run", 2), 100.0);
    let body_out = |mode: &str, level: u32| {
        out.iter()
            .find(|r| r.mode == mode && r.place.level == level)
            .unwrap()
            .result
            .body_out
    };
    assert_eq!(body_out("run", 1), Some(2.0), "Water + the drunk Oil");
    assert_eq!(body_out("level", 2), None, "nobody reached the exit");
    assert_eq!(body_out("run", 2), Some(1.0), "the sun took the Water");
    let s: Vec<_> = summary.iter().filter(|s| s.rule == "min90").collect();
    assert_eq!(s.len(), 2, "one per mode for E strafe, branch Oil");
    assert_eq!(s[0].mode, "level");
    assert_eq!((s[0].furthest, s[0].first_fail), (Some(1), Some(2)));
    assert_eq!(s[0].cause.as_deref(), Some("room1:sun"));
    assert_eq!(
        (s[1].furthest, s[1].first_fail, s[1].branch.as_str()),
        (Some(2), None, "Oil")
    );
}

#[test]
fn ladder_rows_are_checked() {
    let l1 = row(
        "a",
        2,
        "[\"Water\"]",
        "ladder = { level = 1, difficulty = \"E\", branch = \"Oil\" }",
        "",
    );
    let orphan = row(
        "b",
        2,
        "[\"Water\"]",
        "ladder = { level = 2, difficulty = \"E\", branch = \"Acid\" }",
        "",
    );
    let err = design(&(l1.clone() + &orphan)).unwrap_err();
    assert!(err.contains("no level 1 row"), "{err}");
    let twin = row(
        "c",
        2,
        "[\"Water\"]",
        "ladder = { level = 1, difficulty = \"E\", branch = \"Oil\" }",
        "",
    );
    assert!(design(&(l1.clone() + &twin)).unwrap_err().contains("share"));
    let bad = row(
        "d",
        2,
        "[\"Water\"]",
        "ladder = { level = 1, difficulty = \"X\" }",
        "",
    );
    assert!(design(&bad).is_err());
    let river = row("e", 2, "[\"Water\"]", "", ", river = 2");
    assert!(design(&river).unwrap_err().contains("river_liquids"));
}
