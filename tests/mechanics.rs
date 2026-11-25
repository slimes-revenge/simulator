//! Stage 2 of the 20-level graph: enemy traits (G1 dash, G2 smell, G4 alarm, G5 Pus, nets),
//! G3 regeneration only in a fight, the G17 hunter and the squad camp.

use simulator::data::{Design, Liquid, Trait};
use simulator::fight::{Fight, Policy};
use simulator::map;

/// The bundled design with the Mushroom changed by `edit`.
fn mushroom(edit: impl Fn(&mut simulator::data::EnemyData)) -> Design {
    let mut d = Design::bundled();
    edit(d.enemies.iter_mut().find(|e| e.name == "Mushroom").unwrap());
    d
}

/// Runs `rounds` rounds and returns (log, body at the end).
fn play(
    d: &Design,
    body: Vec<Liquid>,
    policy: Policy,
    ap: u32,
    enemies: &[(&str, (i32, i32))],
    rounds: u32,
) -> (Vec<String>, Vec<Liquid>) {
    let mut f = Fight::new(d, 0, ap, body.clone(), (0, 5), enemies);
    f.policy = policy;
    f.max_rounds = rounds;
    let log = f.run_traced().1;
    let mut f = Fight::new(d, 0, ap, body, (0, 5), enemies);
    f.policy = policy;
    f.max_rounds = rounds;
    (log, f.run_with_volume().1)
}

fn has(log: &[String], start: &str) -> bool {
    log.iter().any(|l| l.starts_with(start))
}

#[test]
fn a_dashing_enemy_bites_on_the_turn_it_arrives() {
    let fast = |dash: bool| {
        mushroom(move |e| {
            e.speed = 2;
            e.traits = if dash { vec![Trait::Dash] } else { vec![] };
        })
    };
    let water = vec![Liquid::Water; 10];
    for (dash, left) in [(true, 9), (false, 10)] {
        let (_, body) = play(
            &fast(dash),
            water.clone(),
            Policy::Kite,
            1,
            &[("Mushroom", (3, 5))],
            1,
        );
        assert_eq!(body.len(), left, "dash {dash}");
    }
}

#[test]
fn blood_on_the_edge_lets_smelling_enemies_see_farther() {
    // Vision 5; the Mushroom stands 6 away. With SmellsBlood it sees 7 while Blood is the edge.
    let d = mushroom(|e| e.traits = vec![Trait::SmellsBlood]);
    let mut blood = vec![Liquid::Water; 9];
    blood.push(Liquid::Blood);
    let (log, _) = play(&d, blood, Policy::Kite, 1, &[("Mushroom", (6, 5))], 1);
    assert!(has(&log, "Mushroom#1 Chase"), "{log:?}");
    let (log, _) = play(
        &d,
        vec![Liquid::Water; 10],
        Policy::Kite,
        1,
        &[("Mushroom", (6, 5))],
        1,
    );
    assert!(!has(&log, "Mushroom#1 Chase"), "{log:?}");
}

#[test]
fn an_alarm_wakes_enemies_within_its_radius() {
    // The Eye (5 away) sees the slime; the Mushroom 8 away cannot, but stands 4 from the Eye
    // once it has stepped closer.
    let d = |alarm: bool| {
        let mut d = Design::bundled();
        let eye = d.enemies.iter_mut().find(|e| e.name == "Eye").unwrap();
        eye.traits = if alarm {
            vec![Trait::Alarm { radius: 4 }]
        } else {
            vec![]
        };
        d
    };
    let enemies = [("Eye", (5, 5)), ("Mushroom", (8, 5))];
    let water = vec![Liquid::Water; 10];
    let (log, _) = play(&d(true), water.clone(), Policy::Kite, 1, &enemies, 2);
    assert!(has(&log, "Eye#1 raises the alarm"), "{log:?}");
    assert!(has(&log, "Mushroom#2 Chase"), "{log:?}");
    let (log, _) = play(&d(false), water, Policy::Kite, 1, &enemies, 2);
    assert!(!has(&log, "Mushroom#2 Chase"), "{log:?}");
}

#[test]
fn an_infecting_bite_turns_blood_into_pus_and_pus_is_spat_off() {
    use Liquid::*;
    let d = mushroom(|e| {
        e.traits = vec![Trait::Infects];
        e.hit_points = 99;
    });
    // Round 1: the slime spits the edge Pus; the bite takes the edge Water; the edge-most
    // Blood turns to Pus.
    let (log, body) = play(
        &d,
        vec![Water, Blood, Blood, Water, Pus],
        Policy::Newcomer,
        1,
        &[("Mushroom", (1, 5))],
        1,
    );
    assert!(has(&log, "Slime spits Pus"), "{log:?}");
    assert_eq!(body, vec![Water, Blood, Pus]);
}

#[test]
fn pus_spreads_to_blood_neighbours_and_water_stops_it() {
    use Liquid::*;
    // No bite: only the spread. The strike spends the edge Water; then the Pus turns the Blood
    // above it, and the Water below it shields the deep Blood.
    let d = mushroom(|e| {
        e.melee_power = 0;
        e.hit_points = 99;
    });
    let (_, body) = play(
        &d,
        vec![Blood, Water, Pus, Blood, Water],
        Policy::Newcomer,
        1,
        &[("Mushroom", (1, 5))],
        1,
    );
    assert_eq!(body, vec![Blood, Water, Pus, Pus]);
}

#[test]
fn a_net_keeps_the_slime_from_stepping() {
    let d = |nets: bool| {
        mushroom(move |e| {
            e.traits = if nets { vec![Trait::Nets] } else { vec![] };
            e.hit_points = 99;
        })
    };
    let water = vec![Liquid::Water; 10];
    let (log, _) = play(
        &d(true),
        water.clone(),
        Policy::Kite,
        1,
        &[("Mushroom", (1, 4))],
        3,
    );
    // Netted, kite strikes instead of stepping back.
    assert!(!has(&log, "Slime steps back"), "{log:?}");
    assert!(has(&log, "Slime strikes Mushroom#1"), "{log:?}");
    let (log, _) = play(
        &d(false),
        water,
        Policy::Kite,
        1,
        &[("Mushroom", (1, 4))],
        3,
    );
    assert!(has(&log, "Slime steps back"), "{log:?}");
}

#[test]
fn regeneration_grows_blood_only_in_a_fight() {
    // A Passive Bat far away never engages: no growth under the G3 rule.
    let body = vec![Liquid::Blood; 9];
    let d = Design::bundled();
    assert!(d.rules.regeneration_in_fight_only);
    let (_, out) = play(&d, body.clone(), Policy::Kite, 1, &[("Bat", (9, 9))], 2);
    assert_eq!(out.len(), 9);
    let mut game = Design::bundled();
    game.rules.regeneration_in_fight_only = false;
    let (_, out) = play(&game, body, Policy::Kite, 1, &[("Bat", (9, 9))], 2);
    assert_eq!(out.len(), 10);
}

fn map_design(extra: &str) -> Design {
    let text =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/data/design.toml")).unwrap();
    let mut text = text[..text.find("[[levels]]").unwrap()].to_string();
    text += &format!(
        "[[levels]]\nid = \"m\"\ncapacity = 10\naction_points = 2\npolicy = \"kite\"\n\
         map = {{ rooms = 3, room_side = [5, 7], enemies_per_room = [0, 0], \
         enemy_kinds = [\"Mushroom\"], corridor = 2, vision = 3{extra} }}\n"
    );
    Design::parse(&text).unwrap()
}

#[test]
fn the_hunter_enters_when_the_slime_reaches_the_second_to_last_room() {
    // enter_round is far away: it is the room that calls it in.
    let d = map_design(", hunter = { kind = \"Mushroom\", enter_round = 999 }");
    let level = &d.levels[0];
    let r = map::run(&d, level);
    let h = r.hunter.expect("a hunter level");
    assert!((h.iter().sum::<f64>() - 100.0).abs() < 0.1, "{h:?}");
    for seed in 0..5 {
        let (_, state) = map::play(
            &d,
            level,
            level.map.as_ref().unwrap(),
            &map::start_body(level),
            seed,
        );
        assert!(state.hunter_at.is_some(), "seed {seed}");
        assert!(
            state.episodes.iter().any(|e| e.hunter),
            "seed {seed}: it comes for the slime"
        );
    }
    assert!(r.hunter_net.is_some());
    let none = map_design("");
    assert_eq!(map::run(&none, &none.levels[0]).hunter, None);
}

#[test]
fn the_camp_stands_in_the_second_to_last_room() {
    let d = map_design(", camp = [\"Mushroom\", \"Bat\"]");
    let spec = d.levels[0].map.as_ref().unwrap();
    for seed in 0..10 {
        let m = map::generate(spec, seed);
        let camp: Vec<&str> = m
            .enemies
            .iter()
            .filter(|e| e.2 == Some(1))
            .map(|e| e.0.as_str())
            .collect();
        assert_eq!(camp, vec!["Mushroom", "Bat"], "seed {seed}");
    }
}

#[test]
fn pus_does_not_spread_outside_a_fight() {
    use Liquid::*;
    // A Passive Bat far away never engages: no clock, no spread.
    let d = Design::bundled();
    let body = vec![Blood, Pus, Blood, Water];
    let (_, out) = play(&d, body.clone(), Policy::Kite, 1, &[("Bat", (9, 9))], 3);
    assert_eq!(out, body);
}

#[test]
fn an_alarm_rings_even_when_the_slime_struck_first() {
    // The Eye stands next to the slime; the newcomer strikes it first (it gets aggroed by the
    // hit), then on its own turn it sees the slime and still raises the alarm, once.
    let mut d = Design::bundled();
    let eye = d.enemies.iter_mut().find(|e| e.name == "Eye").unwrap();
    eye.traits = vec![Trait::Alarm { radius: 9 }];
    eye.hit_points = 99;
    let (log, _) = play(
        &d,
        vec![Liquid::Water; 10],
        Policy::Newcomer,
        1,
        &[("Eye", (1, 5)), ("Mushroom", (9, 9))],
        3,
    );
    let alarms = log
        .iter()
        .filter(|l| l.starts_with("Eye#1 raises the alarm"))
        .count();
    assert_eq!(alarms, 1, "{log:?}");
}
