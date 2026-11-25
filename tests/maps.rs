//! Map levels: generation, walls and sight, the strafe walk, episodes.

use simulator::data::{Design, Liquid, MapSpec};
use simulator::fight::Board;
use simulator::map;
use simulator::path;

fn spec(rooms: usize, side: (i32, i32), enemies: (usize, usize), vision: i32) -> MapSpec {
    MapSpec {
        rooms,
        room_side: side,
        enemies_per_room: enemies,
        enemy_kinds: vec!["Mushroom".into(), "Bat".into()],
        corridor: 2,
        vision,
        blockers: Vec::new(),
        blockers_per_level: 0,
    }
}

fn design_with(level_map: &str) -> Design {
    let mut text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += &format!(
        "[[levels]]\nid = \"t\"\ncapacity = 10\naction_points = 1\npolicy = \"strafe\"\nmap = {level_map}\n"
    );
    Design::parse(&text).unwrap()
}

#[test]
fn generated_maps_are_walkable_and_fair() {
    for seed in 0..30 {
        let s = spec(5, (5, 9), (1, 2), 3);
        let m = map::generate(&s, seed);
        assert_eq!(m.rooms.len(), 5);
        assert!(m.board.open(m.start) && m.board.open(m.exit));
        // The exit is reachable through walls alone.
        let route = path::find(m.board.width, m.board.height, m.start, &[m.exit], |c| {
            m.board.wall(c)
        });
        assert!(route.is_some(), "seed {seed}:\n{}", map::ascii(&m));
        for (_, cell, room) in &m.enemies {
            let ((x0, y0), (x1, y1)) = m.rooms[room.unwrap()];
            assert!(cell.0 >= x0 && cell.0 <= x1 && cell.1 >= y0 && cell.1 <= y1);
            assert!(
                path::chebyshev(*cell, m.start) > s.vision,
                "nobody starts aggroed"
            );
        }
        assert_eq!(
            map::ascii(&m),
            map::ascii(&map::generate(&s, seed)),
            "same seed, same map"
        );
    }
}

#[test]
fn walls_block_sight() {
    // . . # . .
    let board = Board {
        width: 5,
        height: 1,
        walls: vec![false, false, true, false, false],
    };
    assert!(!board.line_of_sight((0, 0), (4, 0)));
    assert!(board.line_of_sight((0, 0), (1, 0)));
    assert!(board.line_of_sight((3, 0), (4, 0)));
    // The wall itself is visible (endpoints do not block).
    assert!(board.line_of_sight((0, 0), (2, 0)));
}

#[test]
fn an_empty_room_is_a_straight_walk() {
    // One 5×5 room, no enemies: start (1, 3), exit (5, 3) — 4 steps at 1 action a round.
    let d = design_with(
        r#"{ rooms = 1, room_side = [5, 5], enemies_per_room = [0, 0], enemy_kinds = ["Mushroom"], corridor = 2, vision = 3 }"#,
    );
    let r = map::run(&d, &d.levels[0]);
    assert_eq!((r.end, r.rooms, r.rounds, r.fights), (100.0, 1.0, 4.0, 0.0));
    assert_eq!(r.net_per_fight, None);
    assert_eq!((r.min_body, r.worst_body), (10.0, 10));
}

#[test]
fn episodes_account_for_the_whole_body() {
    // Water does nothing over time and no Blood grows, so outside episodes the body never
    // changes: the end body = start + Σ net − Σ wasted, in every seed.
    let d = design_with(
        r#"{ rooms = 3, room_side = [5, 7], enemies_per_room = [1, 2], enemy_kinds = ["Mushroom", "Eye"], corridor = 1, vision = 3 }"#,
    );
    let level = &d.levels[0];
    let spec = level.map.as_ref().unwrap();
    let body = vec![Liquid::Water; 10];
    let mut episodes = 0;
    for seed in 0..20 {
        let (result, state) = map::play(&d, level, spec, &body, seed);
        let net: i64 = state.episodes.iter().map(|e| e.net).sum();
        let wasted: i64 = state.episodes.iter().map(|e| e.wasted as i64).sum();
        assert_eq!(
            result.units_left as i64,
            10 + net - wasted,
            "seed {seed}: {state:?}"
        );
        episodes += state.episodes.len();
        for e in &state.episodes {
            assert_eq!(e.corpse, e.eaten + e.wasted);
        }
    }
    assert!(episodes > 0);
    let r = map::run(&d, level);
    let shares: f64 = r.engaged.iter().sum();
    assert!((shares - 100.0).abs() < 0.05, "{r:?}");
}

#[test]
fn map_level_errors_are_explicit() {
    let base =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let both = base.replacen(
        "policy = \"strafe\"\nmap",
        "policy = \"strafe\"\nstages = [{ kind = \"fountain\", liquid = \"Water\", units = 1 }]\nmap",
        1,
    );
    assert!(
        Design::parse(&both)
            .unwrap_err()
            .contains("either stages or map")
    );
    let face = base.replacen("policy = \"strafe\"", "policy = \"newcomer\"", 1);
    assert!(Design::parse(&face).unwrap_err().contains("strafe"));
    let lure_on_stages = base.replacen("policy = \"newcomer\"", "policy = \"lure\"", 1);
    assert!(
        Design::parse(&lure_on_stages)
            .unwrap_err()
            .contains("newcomer")
    );
    let ghost = base.replacen("enemy_kinds = [\"Bat\"", "enemy_kinds = [\"Ghost\"", 1);
    assert!(
        Design::parse(&ghost)
            .unwrap_err()
            .contains("no enemy named Ghost")
    );
}

#[test]
fn sight_lines_bend_around_corners() {
    // . . .
    // . # .
    // . . .
    let board = Board {
        width: 3,
        height: 3,
        walls: vec![false, false, false, false, true, false, false, false, false],
    };
    assert!(
        !board.line_of_sight((0, 0), (2, 2)),
        "the diagonal passes the wall"
    );
    assert!(!board.line_of_sight((0, 1), (2, 1)));
    assert!(board.line_of_sight((0, 0), (2, 0)), "the top row is clear");
    assert!(board.line_of_sight((0, 0), (1, 0)));
}

#[test]
fn calm_enemies_stay_in_their_rooms() {
    // Passive bats never notice a slime parked out of reach, so they only wander: every step
    // they take in the log ends inside their own room.
    let d = design_with(
        r#"{ rooms = 3, room_side = [5, 6], enemies_per_room = [2, 2], enemy_kinds = ["Bat"], corridor = 1, vision = 1 }"#,
    );
    let s = d.levels[0].map.clone().unwrap();
    let mut steps = 0;
    for seed in 0..5 {
        let m = map::generate(&s, seed);
        let mut f = map::fight(&d, &d.levels[0], &s, &[Liquid::Water; 10], seed);
        f.slime.cell = (0, 0); // inside the outer wall: no path, never seen
        f.max_rounds = 60;
        let (_, log) = f.run_traced();
        for line in log
            .iter()
            .filter(|l| l.starts_with("Bat#") && l.contains("->("))
        {
            let k: usize = line[4..line.find(' ').unwrap()].parse().unwrap();
            let to = &line[line.find("->(").unwrap() + 3..line.find("), slime").unwrap()];
            let (x, y) = to.split_once(", ").unwrap();
            let (x, y): (i32, i32) = (x.parse().unwrap(), y.parse().unwrap());
            let ((x0, y0), (x1, y1)) = m.rooms[m.enemies[k - 1].2.unwrap()];
            assert!(
                x >= x0 && x <= x1 && y >= y0 && y <= y1,
                "seed {seed}: {line}"
            );
            steps += 1;
        }
    }
    assert!(steps > 0);
}

#[test]
fn lure_meets_enemies_one_at_a_time_more_often_than_strafe() {
    // Two rooms, two Mushrooms waiting in the second. Strafe walks in and meets both; lure
    // waits at the doorway and fights from the corridor.
    let level = |policy: &str| {
        let mut d = design_with(
            r#"{ rooms = 2, room_side = [6, 7], enemies_per_room = [0, 2], enemy_kinds = ["Mushroom"], corridor = 2, vision = 3 }"#,
        );
        d.levels[0].policy = policy.into();
        d.matrix.seeds = 40;
        d
    };
    let share_of_two = |d: &Design| {
        let r = map::run(d, &d.levels[0]);
        assert!(r.fights > 0.0, "{r:?}");
        r.engaged[2] + r.engaged[3]
    };
    let (strafe, lure) = (share_of_two(&level("strafe")), share_of_two(&level("lure")));
    assert!(
        lure < strafe,
        "lure {lure}% vs strafe {strafe}% of fights with 2+ at once"
    );
}

#[test]
fn lure_waits_at_most_its_patience_per_approach() {
    let waits = |patience: u32, seed: u64| {
        let mut d = design_with(
            r#"{ rooms = 2, room_side = [6, 7], enemies_per_room = [0, 2], enemy_kinds = ["Mushroom"], corridor = 2, vision = 4 }"#,
        );
        d.levels[0].policy = "lure".into();
        d.policies.lure_patience = patience;
        let s = d.levels[0].map.clone().unwrap();
        let (_, log) = map::fight(&d, &d.levels[0], &s, &[Liquid::Water; 10], seed).run_traced();
        log.iter()
            .filter(|l| l.starts_with("Slime waits at the doorway"))
            .count()
    };
    let total: usize = (0..20).map(|seed| waits(6, seed)).sum();
    assert!(
        total > 0,
        "two calm Mushrooms should hold the slime at the doorway sometimes"
    );
    for seed in 0..20 {
        assert_eq!(waits(0, seed), 0);
        // Two rooms: one approach into room 2.
        assert!(waits(6, seed) <= 6, "seed {seed}");
    }
}

fn level2_map(policy: &str) -> Design {
    let mut d = design_with(
        r#"{ rooms = 4, room_side = [5, 7], enemies_per_room = [0, 0], enemy_kinds = ["Mushroom"], corridor = 2, vision = 3, blockers = ["Cactus"], blockers_per_level = 3 }"#,
    );
    let c = d.enemies.iter_mut().find(|e| e.name == "Cactus").unwrap();
    (c.hit_points, c.thorns, c.speed, c.melee_power) = (2, 1, 0, 0);
    d.levels[0].policy = policy.into();
    d.matrix.seeds = 10;
    d
}

#[test]
fn blockers_stand_in_passages() {
    let d = level2_map("strafe");
    let s = d.levels[0].map.clone().unwrap();
    for seed in 0..10 {
        let m = map::generate(&s, seed);
        let blockers: Vec<_> = m.enemies.iter().filter(|e| e.2.is_none()).collect();
        assert_eq!(blockers.len(), 3);
        for (kind, cell, _) in blockers {
            assert_eq!(kind, "Cactus");
            assert!(m.board.open(*cell));
            assert!(m.rooms.iter().all(|&((x0, y0), (x1, y1))| {
                cell.0 < x0 || cell.0 > x1 || cell.1 < y0 || cell.1 > y1
            }));
        }
    }
}

#[test]
fn strafe_pays_thorns_where_lure_spits() {
    // Three Cactus blockers in the passages and nothing else: strafe pays a thorn per strike,
    // lure spits and pays none.
    let thorns = |policy: &str| {
        let d = level2_map(policy);
        let level = &d.levels[0];
        let s = level.map.clone().unwrap();
        (0..10)
            .map(|seed| {
                let (r, _) = map::play(&d, level, &s, &[Liquid::Water; 10], seed);
                // Each Cactus costs strafe 4 units (2 strikes + 2 thorns): 3 of them kill a
                // 10-unit slime on the third, as balance counted by hand. Lure always passes.
                if policy == "lure" {
                    assert!(r.won, "seed {seed}: {r:?}");
                } else {
                    assert_eq!(
                        r.cause.as_deref(),
                        Some("thorns:Cactus"),
                        "seed {seed}: {r:?}"
                    );
                }
                r.units_thorns
            })
            .sum::<usize>()
    };
    assert_eq!(thorns("lure"), 0);
    assert!(thorns("strafe") > 0);
}
