//! One fight by the game's rules: board, creatures, statuses, strikes, enemy AI, turn order.
//!
//! Each function names the C# method it mirrors (slimes-revenge/SlimesRevenge,
//! `Assets/Scripts/Runtime`). Rules notes: `docs/rules-model.md`.

use crate::data::{Design, Liquid, Personality, Trait};
use crate::path::{self, Cell, chebyshev};
use crate::rng::Rng;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusKind {
    Wet,
    Burning,
    Poisoned,
    Corroding {
        corrosion: i32,
    },
    Instability,
    Flammable,
    /// Blood dominance: +Blood each pulse; cleared when dominance changes.
    Regeneration,
}

#[derive(Clone, Copy, Debug)]
pub struct Status {
    pub kind: StatusKind,
    /// Pulses left; `FOREVER` never expires (`StatusEffect.Forever`).
    pub remaining: i32,
    id: u32,
}

pub const FOREVER: i32 = -1;

#[derive(Clone, Debug)]
pub struct Creature {
    pub name: String,
    pub is_slime: bool,
    pub cell: Cell,
    pub hit_points: i32,
    pub max_hit_points: i32,
    pub armor: i32,
    pub base_speed: i32,
    pub melee_power: i32,
    /// The slime's body and life; for enemies, the corpse's units.
    pub volume: Vec<Liquid>,
    /// `StatusQueue`, FIFO. Same kinds stack: two Burnings both pulse.
    pub statuses: Vec<Status>,
    pub personality: Personality,
    pub vampirism: bool,
    /// Slime only: the liquid with at least `dominance_percent` of the volume.
    pub dominant: Option<Liquid>,
    pub aggroed: bool,
    pub pursuit: Option<Cell>,
    pub pursuit_left: i32,
    pub flee_cell: Option<Cell>,
}

impl Creature {
    pub fn alive(&self) -> bool {
        if self.is_slime {
            !self.volume.is_empty()
        } else {
            self.hit_points > 0
        }
    }

    /// `Creature.Speed`: base (at least 1) plus status modifiers, never below 0.
    fn speed(&self, d: &Design) -> i32 {
        let modifier: i32 = self
            .statuses
            .iter()
            .filter(|s| s.kind == StatusKind::Instability)
            .map(|_| d.statuses.instability_speed)
            .sum();
        (self.base_speed.max(1) + modifier).max(0)
    }

    fn count(&self, liquid: Liquid) -> usize {
        self.volume.iter().filter(|&&l| l == liquid).count()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Intent {
    Attack,
    Chase,
    Flee,
    Wander,
    Idle,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FightResult {
    pub won: bool,
    pub rounds: u32,
    pub units_left: usize,
    pub units_spent: usize,
    /// Units the slime lost to enemy bites.
    pub units_bitten: usize,
    /// Units the slime lost to its own status pulses.
    pub units_status: usize,
    pub enemies_killed: usize,
    /// Why the slime lost: `melee:<Enemy>`, `status:<Status>` or `timeout`. `None` on a win.
    pub cause: Option<String>,
}

pub struct Fight<'a> {
    d: &'a Design,
    rng: Rng,
    actions_per_round: u32,
    pub slime: Creature,
    pub enemies: Vec<Creature>,
    next_status_id: u32,
    units_spent: usize,
    units_bitten: usize,
    units_status: usize,
    /// What last took a unit off the slime.
    last_harm: Option<String>,
    /// Turn-by-turn log lines, kept only for `--trace`.
    trace: Option<Vec<String>>,
}

impl<'a> Fight<'a> {
    /// The slime with `volume` at `slime_cell`; enemies as (kind, cell).
    pub fn new(
        d: &'a Design,
        seed: u64,
        actions_per_round: u32,
        volume: Vec<Liquid>,
        slime_cell: Cell,
        enemies: &[(&str, Cell)],
    ) -> Fight<'a> {
        let slime = Creature {
            name: "Slime".into(),
            is_slime: true,
            cell: slime_cell,
            hit_points: 0,
            max_hit_points: 0,
            armor: 0,
            base_speed: d.slime.speed,
            melee_power: 0,
            volume: volume.into_iter().take(d.slime.capacity).collect(),
            statuses: Vec::new(),
            personality: Personality::Aggressive,
            vampirism: false,
            dominant: None,
            aggroed: false,
            pursuit: None,
            pursuit_left: 0,
            flee_cell: None,
        };
        let enemies = enemies
            .iter()
            .map(|&(kind, cell)| {
                let e = d.enemy(kind);
                Creature {
                    name: e.name.clone(),
                    is_slime: false,
                    cell,
                    hit_points: e.hit_points,
                    max_hit_points: e.hit_points,
                    armor: e.armor,
                    base_speed: e.speed,
                    melee_power: e.melee_power,
                    volume: e.volume.clone(),
                    statuses: Vec::new(),
                    personality: e.personality,
                    vampirism: e.traits.contains(&Trait::Vampirism),
                    dominant: None,
                    aggroed: false,
                    pursuit: None,
                    pursuit_left: 0,
                    flee_cell: None,
                }
            })
            .collect();
        let mut fight = Fight {
            d,
            rng: Rng::new(seed),
            actions_per_round,
            slime,
            enemies,
            next_status_id: 0,
            units_spent: 0,
            units_bitten: 0,
            units_status: 0,
            last_harm: None,
            trace: None,
        };
        let d = fight.d;
        refresh_dominance(d, &mut fight.slime, &mut fight.next_status_id);
        fight
    }

    /// Plays the fight to the end. Round: the slime acts `actions_per_round` times, then each
    /// enemy pulses its statuses and takes one intent, then the slime's statuses pulse
    /// (`GameSession.Resolve`). `TurnManager.Bind` pulses the slime once before its first action.
    pub fn run(self) -> FightResult {
        self.play().0
    }

    /// Plays the fight and returns a turn-by-turn log as well (`--trace`).
    pub fn run_traced(mut self) -> (FightResult, Vec<String>) {
        self.trace = Some(Vec::new());
        self.play()
    }

    fn play(mut self) -> (FightResult, Vec<String>) {
        self.note(|f| format!("start: {}", f.describe_all()));
        let result = self.rounds();
        self.note(|_| format!("result: {result:?}"));
        (result, self.trace.unwrap_or_default())
    }

    fn note(&mut self, line: impl FnOnce(&Fight) -> String) {
        if let Some(mut trace) = self.trace.take() {
            trace.push(line(self));
            self.trace = Some(trace);
        }
    }

    fn label(&self, i: usize) -> String {
        format!("{}#{}", self.enemies[i].name, i + 1)
    }

    fn describe_all(&self) -> String {
        let s = &self.slime;
        let mut out = format!(
            "Slime {:?} volume [{}] dominant {:?} statuses {:?}",
            s.cell,
            s.volume
                .iter()
                .map(|l| format!("{l:?}"))
                .collect::<Vec<_>>()
                .join(" "),
            s.dominant,
            s.statuses
                .iter()
                .map(|x| (x.kind, x.remaining))
                .collect::<Vec<_>>()
        );
        for (i, e) in self.enemies.iter().enumerate().filter(|(_, e)| e.alive()) {
            out += &format!(
                "; {} {:?} HP {}/{} armor {} aggro {} statuses {:?}",
                self.label(i),
                e.cell,
                e.hit_points,
                e.max_hit_points,
                e.armor,
                e.aggroed,
                e.statuses
                    .iter()
                    .map(|x| (x.kind, x.remaining))
                    .collect::<Vec<_>>()
            );
        }
        out
    }

    fn rounds(&mut self) -> FightResult {
        self.pulse(None);
        if !self.slime.alive() {
            return self.result(false, 0);
        }
        for round in 1..=self.d.rules.max_rounds {
            self.note(|_| format!("round {round}"));
            for _ in 0..self.actions_per_round {
                self.slime_act();
                if self.enemies_dead() {
                    return self.result(true, round);
                }
            }
            for i in 0..self.enemies.len() {
                if !self.enemies[i].alive() {
                    continue;
                }
                self.pulse(Some(i));
                if !self.enemies[i].alive() {
                    continue;
                }
                self.enemy_turn(i);
                if !self.slime.alive() {
                    return self.result(false, round);
                }
            }
            if self.enemies_dead() {
                return self.result(true, round);
            }
            self.pulse(None);
            if !self.slime.alive() {
                return self.result(false, round);
            }
            self.note(|f| format!("end of round {round}: {}", f.describe_all()));
        }
        self.last_harm = Some("timeout".into());
        self.result(false, self.d.rules.max_rounds)
    }

    fn result(&self, won: bool, rounds: u32) -> FightResult {
        FightResult {
            won,
            rounds,
            units_left: self.slime.volume.len(),
            units_spent: self.units_spent,
            units_bitten: self.units_bitten,
            units_status: self.units_status,
            enemies_killed: self.enemies.iter().filter(|e| !e.alive()).count(),
            cause: if won { None } else { self.last_harm.clone() },
        }
    }

    fn enemies_dead(&self) -> bool {
        self.enemies.iter().all(|e| !e.alive())
    }

    fn occupied(&self) -> Vec<Cell> {
        std::iter::once(&self.slime)
            .chain(&self.enemies)
            .filter(|c| c.alive())
            .map(|c| c.cell)
            .collect()
    }

    fn inside(&self, cell: Cell) -> bool {
        cell.0 >= 0
            && cell.1 >= 0
            && cell.0 < self.d.rules.board_width
            && cell.1 < self.d.rules.board_height
    }

    /// `CreatureMoves.ApproachCells` → `GridPath.TryWalk`: the cell `speed` steps along the
    /// route to a free cell next to `target`, or `None` when there is no route.
    fn walk_toward(&self, from: Cell, target: Cell, speed: i32) -> Option<Cell> {
        if speed < 1 {
            return None;
        }
        let occupied = self.occupied();
        let free = |c: Cell| self.inside(c) && !occupied.contains(&c);
        let mut goals: Vec<Cell> = path::neighbours(target)
            .filter(|&c| c == from || free(c))
            .collect();
        if goals.is_empty() && free(target) {
            goals.push(target);
        }
        let route = path::find(
            self.d.rules.board_width,
            self.d.rules.board_height,
            from,
            &goals,
            |c| occupied.contains(&c),
        )?;
        Some(route[(speed as usize).min(route.len()) - 1])
    }

    // ---- the slime: face-tank policy -------------------------------------------------------

    /// Face-tank: walk to the nearest enemy, strike it with the liquid the slime has most of,
    /// wait when the last unit cannot be spent. Uses only player actions (step, attack, wait).
    fn slime_act(&mut self) {
        let Some(target) = self
            .enemies
            .iter()
            .enumerate()
            .filter(|(_, e)| e.alive())
            .min_by_key(|(i, e)| (chebyshev(self.slime.cell, e.cell), *i))
            .map(|(i, _)| i)
        else {
            return;
        };
        if chebyshev(self.slime.cell, self.enemies[target].cell) == 1 {
            // `Volume.CanSpend`: the last unit is the slime's life.
            if self.slime.volume.len() > 1 {
                let liquid = self.most_held_liquid();
                let hp = self.enemies[target].hit_points;
                self.slime_strike(target, liquid);
                self.note(|f| {
                    let e = &f.enemies[target];
                    format!(
                        "Slime strikes {} with {liquid:?}: HP {hp}->{}, armor {}, statuses {:?}",
                        f.label(target),
                        e.hit_points,
                        e.armor,
                        e.statuses
                            .iter()
                            .map(|x| (x.kind, x.remaining))
                            .collect::<Vec<_>>()
                    )
                });
            } else {
                self.note(|_| "Slime waits: the last unit cannot be spent".into());
            }
            return;
        }
        let from = self.slime.cell;
        if let Some(to) = self.walk_toward(from, self.enemies[target].cell, 1) {
            self.slime.cell = to;
            self.note(|_| format!("Slime steps {from:?}->{to:?}"));
        } else {
            self.note(|_| "Slime waits: no path".into());
        }
    }

    /// The liquid with the most units; ties go to the earlier one in `[[liquids]]`.
    fn most_held_liquid(&self) -> Liquid {
        let order = self.d.liquids.iter().map(|l| l.name);
        let most = order
            .clone()
            .map(|l| self.slime.count(l))
            .max()
            .unwrap_or(0);
        order
            .into_iter()
            .find(|&l| self.slime.count(l) == most)
            .expect("the slime holds at least one unit")
    }

    /// `TurnManager.TryAttack` → `Combat.Attack(attacker, target, substance)`.
    pub fn slime_strike(&mut self, target: usize, liquid: Liquid) {
        let d = self.d;
        let Some(at) = self.slime.volume.iter().position(|&l| l == liquid) else {
            return;
        };
        self.slime.volume.remove(at);
        self.units_spent += 1;
        refresh_dominance(d, &mut self.slime, &mut self.next_status_id);

        let stats = d.liquid(liquid);
        let enemy = &mut self.enemies[target];
        enemy.armor = (enemy.armor - stats.corrosion).max(0);
        let power = if liquid == Liquid::Lava {
            modify_incoming_harm(d, enemy, stats.power)
        } else {
            stats.power
        };
        damage(d, enemy, power, true, &mut self.next_status_id);
        if enemy.alive() {
            apply_liquid(d, enemy, liquid, &mut self.next_status_id);
        }
        enemy.aggroed = true;
    }

    // ---- enemies ---------------------------------------------------------------------------

    /// `Creature.TakeTurn`: decide by personality, redirect, perform, after-act.
    fn enemy_turn(&mut self, i: usize) {
        let mut intent = self.decide(i);
        // `CreatureHunt.Redirect`: no fear/hate traits in the live roster, so only pursuit memory.
        if matches!(intent, Intent::Wander | Intent::Idle) {
            let e = &mut self.enemies[i];
            e.flee_cell = None;
            if e.pursuit.is_some() && e.pursuit_left > 0 {
                intent = Intent::Chase;
            } else {
                e.pursuit = None;
                e.pursuit_left = 0;
            }
        }
        let (from, volume) = (self.enemies[i].cell, self.slime.volume.len());
        match intent {
            Intent::Attack => self.enemy_melee(i),
            Intent::Chase => self.chase(i),
            Intent::Flee => self.flee(i),
            Intent::Wander => self.wander(i),
            Intent::Idle => {}
        }
        self.note(|f| {
            let e = &f.enemies[i];
            format!(
                "{} {intent:?}: {from:?}->{:?}, slime volume {volume}->{}, HP {}",
                f.label(i),
                e.cell,
                f.slime.volume.len(),
                e.hit_points
            )
        });
        self.after_act(i, intent);
    }

    /// `IsDetectedBy`: an aggroed observer always detects; Mercury dominance hides the slime.
    fn detected(&self, i: usize) -> bool {
        self.slime.alive()
            && (self.enemies[i].aggroed || self.slime.dominant != Some(Liquid::Mercury))
    }

    fn can_see(&self, i: usize) -> bool {
        chebyshev(self.enemies[i].cell, self.slime.cell) <= self.d.rules.vision_range
            && self.detected(i)
    }

    fn adjacent_and_detected(&self, i: usize) -> bool {
        chebyshev(self.enemies[i].cell, self.slime.cell) == 1 && self.detected(i)
    }

    /// `CreaturePolicyGraphs`: the Aggressive / Passive / Cowardly trees.
    fn decide(&mut self, i: usize) -> Intent {
        let e = &self.enemies[i];
        match e.personality {
            Personality::Aggressive => self.aggressive_tree(i),
            Personality::Passive if e.aggroed => self.aggressive_tree(i),
            Personality::Passive => self.wander_or_idle(),
            Personality::Cowardly => {
                if e.aggroed && self.adjacent_and_detected(i) {
                    return Intent::Attack;
                }
                self.enemies[i].aggroed = false;
                if self.can_see(i) {
                    Intent::Flee
                } else {
                    self.wander_or_idle()
                }
            }
        }
    }

    fn aggressive_tree(&mut self, i: usize) -> Intent {
        if self.adjacent_and_detected(i) {
            Intent::Attack
        } else if self.enemies[i].aggroed || self.can_see(i) {
            Intent::Chase
        } else {
            self.wander_or_idle()
        }
    }

    fn wander_or_idle(&mut self) -> Intent {
        if self.rng.wander_rather_than_idle() {
            Intent::Wander
        } else {
            Intent::Idle
        }
    }

    /// `Combat.Attack(attacker, target)`: a bare melee hit on the slime.
    fn enemy_melee(&mut self, i: usize) {
        if !self.detected(i) {
            return;
        }
        let d = self.d;
        // Retaliation is captured before the hit: the hit may end dominance.
        let retort = self.slime.dominant;
        let power = self.enemies[i].melee_power;
        let before = self.slime.volume.len();
        let tip = damage(d, &mut self.slime, power, true, &mut self.next_status_id);
        self.units_bitten += before - self.slime.volume.len();
        if tip.is_some() {
            self.last_harm = Some(format!("melee:{}", self.enemies[i].name));
        }
        let enemy = &mut self.enemies[i];
        if self.slime.alive() {
            if let Some(liquid) = retort {
                apply_liquid(d, enemy, liquid, &mut self.next_status_id);
            }
            enemy.aggroed = true;
            enemy.pursuit = Some(self.slime.cell);
            enemy.pursuit_left = d.rules.pursuit_memory_turns;
        }
        // `Vampirism.OnOwnerStruckVolumeTip`: runs even when the hit killed the slime.
        if tip == Some(Liquid::Blood) && enemy.vampirism {
            heal(enemy, d.statuses.vampirism_heal);
        }
    }

    /// `CreatureMoves.TryChase`.
    fn chase(&mut self, i: usize) {
        let target = if self.can_see(i) {
            let e = &mut self.enemies[i];
            e.pursuit = Some(self.slime.cell);
            e.pursuit_left = self.d.rules.pursuit_memory_turns;
            self.slime.cell
        } else {
            self.enemies[i].pursuit.unwrap_or(self.slime.cell)
        };
        let e = &self.enemies[i];
        if let Some(to) = self.walk_toward(e.cell, target, e.speed(self.d)) {
            self.enemies[i].cell = to;
        }
    }

    /// `CreatureMoves.TryWander`: one random free neighbour.
    fn wander(&mut self, i: usize) {
        if self.enemies[i].speed(self.d) < 1 {
            return;
        }
        let occupied = self.occupied();
        let mut options: Vec<Cell> = path::neighbours(self.enemies[i].cell)
            .filter(|&c| self.inside(c) && !occupied.contains(&c))
            .collect();
        if options.is_empty() {
            return;
        }
        options.sort();
        self.enemies[i].cell = options[self.rng.pick(options.len())];
    }

    /// `CreatureMoves.TryFlee`: walk toward the free cell farthest from the slime; if that
    /// route does not gain distance, take any neighbour that does.
    fn flee(&mut self, i: usize) {
        let threat = self.slime.cell;
        let occupied = self.occupied();
        let from = self.enemies[i].cell;
        let speed = self.enemies[i].speed(self.d);
        let flee_cell_ok = self.enemies[i]
            .flee_cell
            .is_some_and(|c| !occupied.contains(&c) && c != from);
        if !flee_cell_ok {
            // `CreatureHunt.PickFleeCell`: far from the threat, then near to self.
            let mut best = (from, -1);
            for y in 0..self.d.rules.board_height {
                for x in 0..self.d.rules.board_width {
                    let c = (x, y);
                    if occupied.contains(&c) {
                        continue;
                    }
                    let score = chebyshev(c, threat) * 1000 - chebyshev(c, from);
                    if score > best.1 {
                        best = (c, score);
                    }
                }
            }
            self.enemies[i].flee_cell = Some(best.0);
        }
        let goal = self.enemies[i].flee_cell.unwrap_or(from);
        if goal == from || speed < 1 {
            return;
        }
        let distance = chebyshev(from, threat);
        let route = path::find(
            self.d.rules.board_width,
            self.d.rules.board_height,
            from,
            &[goal],
            |c| occupied.contains(&c),
        );
        if let Some(route) = route {
            let to = route[(speed as usize).min(route.len()) - 1];
            if chebyshev(to, threat) > distance {
                self.enemies[i].cell = to;
                return;
            }
        }
        let away: Vec<Cell> = (-1..=1)
            .flat_map(|y| (-1..=1).map(move |x| (from.0 + x, from.1 + y)))
            .filter(|&c| c != from && self.inside(c) && !occupied.contains(&c))
            .filter(|&c| chebyshev(c, threat) > distance)
            .collect();
        if !away.is_empty() {
            self.enemies[i].cell = away[self.rng.pick(away.len())];
        }
    }

    /// `CreatureHunt.AfterAct` for the slime as focus: sticky aggro and pursuit memory.
    fn after_act(&mut self, i: usize, intent: Intent) {
        let sees = self.can_see(i);
        let adjacent = chebyshev(self.enemies[i].cell, self.slime.cell) == 1;
        let slime_cell = self.slime.cell;
        let memory = self.d.rules.pursuit_memory_turns;
        let e = &mut self.enemies[i];
        match intent {
            Intent::Flee => {
                if e.flee_cell == Some(e.cell) {
                    e.flee_cell = None;
                }
                if adjacent {
                    e.flee_cell = None;
                    e.aggroed = true;
                }
            }
            Intent::Chase | Intent::Attack => {
                if intent == Intent::Attack || sees {
                    e.aggroed = true;
                }
                if sees {
                    e.pursuit = Some(slime_cell);
                    e.pursuit_left = memory;
                } else if let Some(p) = e.pursuit {
                    if chebyshev(e.cell, p) <= 1 {
                        e.pursuit = None;
                        e.pursuit_left = 0;
                    } else {
                        e.pursuit_left -= 1;
                        if e.pursuit_left <= 0 {
                            e.pursuit = None;
                            e.pursuit_left = 0;
                        }
                    }
                }
            }
            Intent::Wander | Intent::Idle => {}
        }
    }

    // ---- statuses --------------------------------------------------------------------------

    /// `StatusQueue.Pulse` for the slime (`None`) or an enemy: each status once, FIFO, at the
    /// start of its owner's turn; stops when the owner dies.
    fn pulse(&mut self, enemy: Option<usize>) {
        let d = self.d;
        let who = enemy.map_or("Slime".to_string(), |i| self.label(i));
        let mut log: Option<Vec<String>> = self.trace.as_ref().map(|_| Vec::new());
        let ids = &mut self.next_status_id;
        let owner = match enemy {
            Some(i) => &mut self.enemies[i],
            None => &mut self.slime,
        };
        let mut pulsed: Vec<u32> = Vec::new();
        while pulsed.len() < owner.statuses.len() + 8 {
            let Some(at) = owner.statuses.iter().position(|s| !pulsed.contains(&s.id)) else {
                break;
            };
            let status = owner.statuses[at];
            pulsed.push(status.id);
            let (before, hp) = (owner.volume.len(), owner.hit_points);
            match status.kind {
                StatusKind::Burning => {
                    let amount = modify_incoming_harm(d, owner, d.statuses.burning_pulse);
                    damage(d, owner, amount, false, ids);
                }
                StatusKind::Poisoned => {
                    damage(d, owner, d.statuses.poisoned_pulse, false, ids);
                }
                StatusKind::Corroding { corrosion } => {
                    if owner.armor > 0 {
                        owner.armor = (owner.armor - corrosion).max(0);
                    } else {
                        damage(d, owner, d.statuses.corroding_pulse, false, ids);
                    }
                }
                StatusKind::Regeneration => {
                    for _ in 0..d.statuses.regeneration_growth {
                        if owner.volume.len() < d.slime.capacity {
                            owner.volume.push(Liquid::Blood);
                        }
                    }
                }
                StatusKind::Wet | StatusKind::Instability | StatusKind::Flammable => {}
            }
            if owner.is_slime && owner.volume.len() < before {
                self.units_status += before - owner.volume.len();
                self.last_harm = Some(format!("status:{:?}", status.kind));
            }
            if let Some(log) = log.as_mut() {
                log.push(format!(
                    "{who} pulse {:?}: HP {hp}->{}, volume {before}->{}, armor {}",
                    status.kind,
                    owner.hit_points,
                    owner.volume.len(),
                    owner.armor
                ));
            }
            // A pulse may have removed the status (dominance change); tick it where it is now.
            if let Some(now) = owner.statuses.iter().position(|s| s.id == status.id) {
                let s = &mut owner.statuses[now];
                if s.remaining > 0 {
                    s.remaining -= 1;
                }
                if s.remaining == 0 {
                    owner.statuses.remove(now);
                }
            }
            if !owner.alive() {
                break;
            }
            refresh_dominance(d, owner, ids);
        }
        if let (Some(trace), Some(log)) = (self.trace.as_mut(), log) {
            trace.extend(log);
        }
    }
}

/// `Creature.Damage`: the slime loses units from the end of its volume (returns the first one
/// lost, the "tip"); others lose hit points, reduced by armor when `blocked_by_armor`.
fn damage(
    d: &Design,
    c: &mut Creature,
    amount: i32,
    blocked_by_armor: bool,
    ids: &mut u32,
) -> Option<Liquid> {
    if amount <= 0 || !c.alive() {
        return None;
    }
    if c.is_slime {
        let mut tip = None;
        for _ in 0..amount {
            let Some(unit) = c.volume.pop() else { break };
            tip.get_or_insert(unit);
        }
        if c.alive() {
            refresh_dominance(d, c, ids);
        }
        return tip;
    }
    let dealt = if blocked_by_armor {
        amount - c.armor.min(amount)
    } else {
        amount
    };
    c.hit_points = (c.hit_points - dealt).max(0);
    None
}

fn heal(c: &mut Creature, amount: i32) {
    if c.alive() {
        c.hit_points = (c.hit_points + amount).min(c.max_hit_points);
    }
}

/// `Creature.ModifyIncomingHarm`: ×multiplier per Flammable in the queue, then the Oil
/// dominance's Forever Flammable unless a Flammable is already queued.
fn modify_incoming_harm(d: &Design, c: &Creature, mut amount: i32) -> i32 {
    if amount <= 0 {
        return amount;
    }
    let mut queued = false;
    for s in &c.statuses {
        if s.kind == StatusKind::Flammable {
            amount *= d.statuses.flammable_multiplier;
            queued = true;
        }
    }
    if c.dominant == Some(Liquid::Oil) && !queued {
        amount *= d.statuses.flammable_multiplier;
    }
    amount
}

/// `Substance.Apply`: what a liquid leaves on whoever it touches (strike, retaliation, puddle),
/// then `NotifyReceivedSubstance` (Vampirism drinks Blood).
fn apply_liquid(d: &Design, target: &mut Creature, liquid: Liquid, ids: &mut u32) {
    let s = &d.statuses;
    match liquid {
        Liquid::Water => add_status(target, StatusKind::Wet, s.wet, ids),
        Liquid::Oil => {
            add_status(target, StatusKind::Instability, s.instability, ids);
            add_status(target, StatusKind::Flammable, s.flammable, ids);
        }
        Liquid::Poison => add_status(target, StatusKind::Poisoned, s.poisoned, ids),
        Liquid::Acid => {
            let corrosion = d.liquid(Liquid::Acid).corrosion;
            add_status(
                target,
                StatusKind::Corroding { corrosion },
                s.corroding,
                ids,
            );
        }
        Liquid::Lava => add_status(target, StatusKind::Burning, s.burning, ids),
        Liquid::Blood | Liquid::Mercury => {}
    }
    if liquid == Liquid::Blood && target.vampirism {
        heal(target, s.vampirism_heal);
    }
}

/// `StatusQueue.Add`: Water dominance blocks Burning; Wet and Burning/Corroding cancel each
/// other (the existing one is removed and the incoming one is not added).
fn add_status(c: &mut Creature, kind: StatusKind, duration: i32, ids: &mut u32) {
    if kind == StatusKind::Burning && c.dominant == Some(Liquid::Water) {
        return;
    }
    let cancels = |existing: StatusKind| {
        matches!(
            (existing, kind),
            (StatusKind::Burning, StatusKind::Wet)
                | (StatusKind::Corroding { .. }, StatusKind::Wet)
                | (StatusKind::Wet, StatusKind::Burning)
        )
    };
    let before = c.statuses.len();
    c.statuses.retain(|s| !cancels(s.kind));
    if c.statuses.len() < before {
        return;
    }
    *ids += 1;
    c.statuses.push(Status {
        kind,
        remaining: duration,
        id: *ids,
    });
}

/// `VolumeDominance.Refresh` (slime only): on a change of dominant liquid, statuses owned by
/// the old one go (Regeneration); Blood queues Regeneration first; Water puts out Burning.
fn refresh_dominance(d: &Design, c: &mut Creature, ids: &mut u32) {
    if !c.is_slime {
        return;
    }
    let mut best: Option<(Liquid, usize)> = None;
    for &unit in &c.volume {
        let n = c.count(unit);
        if best.is_none_or(|(_, b)| n > b) {
            best = Some((unit, n));
        }
    }
    let next = best
        .filter(|&(_, n)| n * 100 >= c.volume.len() * d.rules.dominance_percent)
        .map(|(l, _)| l);
    if next == c.dominant {
        return;
    }
    c.statuses.retain(|s| s.kind != StatusKind::Regeneration);
    c.dominant = next;
    match next {
        Some(Liquid::Blood) => {
            *ids += 1;
            c.statuses.insert(
                0,
                Status {
                    kind: StatusKind::Regeneration,
                    remaining: FOREVER,
                    id: *ids,
                },
            );
        }
        Some(Liquid::Water) => c.statuses.retain(|s| s.kind != StatusKind::Burning),
        _ => {}
    }
}
