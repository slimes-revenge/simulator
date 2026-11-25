# Rules model

How the simulator plays a fight, and how that maps to the game code
(`slimes-revenge/SlimesRevenge`, `Assets/Scripts/Runtime`). The game code is the source of
truth for how the game behaves today; the simulator mirrors it by hand. Numbers live in
`data/design.toml`, rules in `src/fight.rs`.

## Board and movement

- Matrix fights: `rules.board_width` × `rules.board_height` grass, no walls, no puddles. Map
  levels have walls that block movement and sight ([map-levels.md](map-levels.md)).
- 8 directions, one cell per step. Occupied cells block; dead enemies leave the board (their
  corpse does not occupy the cell).
- Pathing (`src/path.rs`) stands in for `GridPath` (A* Pathfinding Project): octile costs
  (straight 10, diagonal 14); among goal cells the route with the fewest cells wins, then the
  cheaper, then the goal with the smaller (x, y). The game breaks ties between equal routes
  by A*'s internals, so a creature may pick a different but equally short cell.
- `Creature.Speed` = base (at least 1) + `statuses.instability_speed` per Instability, never
  below 0. A creature at speed 0 can still attack but does not chase or wander.

## Turn order (`GameSession.Resolve`, `TurnManager`)

1. Before the first round the slime's statuses pulse once (`TurnManager.Bind`).
2. Each round: the slime acts `actionsPerRound` times (every action costs one).
3. Each living enemy in placement order: its statuses pulse (it may die and skip its turn),
   then it chooses one intent and carries it out. The fight is lost the moment the slime dies.
4. The slime's statuses pulse.
5. The fight is won when every enemy is dead (checked after each slime action and after the
   enemies' turns); after `rules.max_rounds` rounds it ends as a `timeout` loss.

## Volume, strikes, damage

- The slime's volume is its life (`Volume.Capacity` = `slime.capacity`); it is alive while it
  has a unit. It cannot spend its last unit (`Volume.CanSpend`).
- **The body is ordered from the edge to the centre** (design rule S13; the `Vec`'s end is
  the edge). A strike spends the edge unit. Bites and status losses also take from the edge.
  New units (eaten, drunk, Regeneration's Blood) land on the edge. `swap` exchanges two
  neighbouring units (positions counted from the edge) for one action.
- A strike (`Combat.Attack`) removes the edge unit, strips `corrosion`
  armor, deals `power` (Lava: multiplied by the target's Flammable, below) through the
  remaining armor as flat reduction, then — if the target lives — applies the liquid's effect
  and the target becomes aggroed.
- Enemy melee is a bare hit of `melee_power`. It pops the edge unit (the "tip"): the starting
  mix (Water, Water, Oil, Poison, Acid, Lava from the centre out) has Lava on the edge.
- Retaliation: when the slime has a dominant liquid before the hit and survives it, the
  attacker receives that liquid's effect.
- Dominance: one liquid with at least `rules.dominance_percent` of the volume.
  Water: blocks and removes Burning. Oil: the slime is Flammable. Blood: Regeneration
  (`statuses.regeneration_growth` Blood per pulse up to capacity, queued first, removed when
  dominance changes). Mercury: Invisible — an enemy that is not aggroed cannot see, chase or
  bite the slime.

## Liquid effects (`Substance.Apply`, on strike and retaliation)

| Liquid | Effect |
| --- | --- |
| Water | Wet (`statuses.wet`) |
| Oil | Instability (`statuses.instability`) and Flammable (`statuses.flammable`) |
| Poison | Poisoned: `statuses.poisoned_pulse` HP per pulse |
| Acid | Corroding: strips `corrosion` armor per pulse, else `statuses.corroding_pulse` HP |
| Lava | Burning: `statuses.burning_pulse` HP per pulse × Flammable |
| Blood | Nothing; a Vampirism creature heals `statuses.vampirism_heal` |
| Mercury | Nothing |

## Statuses (`StatusQueue`)

- FIFO queue, each status pulses once at the start of its owner's turn, then loses one turn;
  it expires at 0. Damage over time ignores armor.
- **Statuses stack.** `StatusQueue.Add` does not merge kinds: three Lava strikes are three
  Burnings, each pulsing. Each Flammable doubles again (two Oil strikes: ×4).
- Wet cancels Burning and Corroding, Burning cancels Wet: the existing status is removed and the
  incoming one is not added.
- Vampirism (innate): heals on receiving Blood and when its bite pops a Blood unit off the slime
  (even the last one).

## Enemy AI (`CreaturePolicyGraphs`, `CreatureHunt`, `CreatureMoves`)

- Sees the slime within Chebyshev `rules.vision_range` unless Mercury hides it.
- **Aggressive:** adjacent → Attack; aggroed or sees → Chase; else wander or idle (coin flip).
- **Passive:** wander or idle until hit, then Aggressive for good.
- **Cowardly:** aggroed and adjacent → Attack; else drop aggro, Flee if it sees the slime.
- Chase walks `speed` cells toward a free cell next to the slime (or its last seen cell for
  `rules.pursuit_memory_turns` turns). Wander steps to a random free neighbour.
- Mob ↔ mob fear and hate (`Fears*`, `Hates*`) are not modelled: only retired animals have them.

## Status sources

Every status remembers where it came from: a slime strike, Retaliation, or the slime's own body
(Regeneration). Rules ignore the source; the `[damage]` slice uses it to split status damage
into `status` and `retal`.

## Newcomer policy (`src/fight.rs`, `slime_act`)

Target the nearest living enemy (Chebyshev, ties to the earlier one). Adjacent: strike with the
edge unit, or wait when only one unit is left. Otherwise step one cell along the path toward
it. It never swaps.

## Arranger policy (`Policy::Arranger`, `arranger_swap`)

Like the newcomer, but before each strike on an adjacent target it may spend one action on
`swap(0, 1)` (the edge and the unit under it). It applies the first matching rule, then
strikes with the edge unit on its next action, whatever the rules say then (one swap per
strike: two rules cannot undo each other's swap):

1. **Keep the kept liquid deep** (`policies.arranger_keep`, or `none`): swap when the edge is
   the kept liquid and the unit under it is not. When both are the kept liquid, it strikes
   with it (one swap, never a bubble).
2. **No water on fire:** swap when the edge is Water, the target has Burning or Corroding, and
   the unit under it is not Water.
3. **Lava on Flammable:** swap when the unit under the edge is Lava, the edge is not, and the
   target is Flammable.

When the swap is the round's last action, the enemies act before the strike. An enemy bite pops
the edge, which is the unit just brought up. At 2 actions per round this wastes many swaps; a
round needs a third action to swap and strike together. With 2 actions the policy strikes
twice even when the enemy is already doomed by damage over time: it spends body it did not
need to.

## Kite policy (`Policy::Kite`, `kite_adjacent`)

For slow enemies (speed 1: a step or a bite per turn). Per slime action:

1. No living enemy aggroed: wait (matrix and stage levels). On map levels it walks like `lure`.
2. Aggroed enemies but none adjacent: wait, so the enemy spends its move coming. On map levels
   it holds `lure`'s fighting spot.
3. Enemies adjacent:
   - **On the round's last action:** step to a free neighbour that no living enemy touches. The
     cell with the largest smallest distance to the enemies wins, then the one farther from
     the nearest room's centre (map levels), then the lowest (x, y). With no such cell, it strikes instead.
   - **Otherwise:** strike the adjacent enemy with the lowest HP (ties: placement order) with
     the edge unit. When the last unit cannot be spent, it steps away as above.

At 2 actions per round that is strike, step. At 4 it is strike ×3, step. A Passive enemy that
nobody hits never aggroes, so against bats the kite waits until the round limit (`timeout`).

## Last-drop strike (`rules.last_drop_strike`)

An agreed design rule (balance П15), on in `data/design.toml`. With the flag on, the slime may
strike with its last unit: the strike hits as usual and the unit is not spent. It applies to
every policy. The game does not implement it yet: there the last unit cannot be spent
(`Volume.CanSpend`), which `last_drop_strike = false` models. Damage from free last-drop strikes counts in
`[damage]`, so `per_unit` (damage per unit spent) reads higher with the flag on.

## Level 2: spit, Thorns, stationary enemies

- **Spit** (`slime_spit`, `rules.spit_range` = 1): the edge unit hits a neighbour like a strike
  (damage, effect, aggro) without melee contact, so Thorns do not answer. One action. `lure`
  and `kite` spit at targets with Thorns; `newcomer`, `arranger` and `strafe` strike in melee.
- **Thorns** (`[[enemies]] thorns`): each melee strike on the creature, the killing one included,
  costs the slime `thorns` more edge units after the strike (`thorns` in the reports). A death
  this way has the cause `thorns:<Enemy>`.
- **Speed 0** (`[[enemies]] speed = 0`): the creature never moves or wanders. The game clamps
  speed to at least 1 (`Creature.SetSpeed`); level 2's stationary Cactus is a design change.
- **No bite** (`melee_power = 0`): its Attack does nothing and does not count as an attack (no
  aggro from it, no Retaliation). `kite` does not step away from such a neighbour; it hits it.

## The 20-level graph: enemy traits, Pus, regeneration in a fight

Agreed design rules for the balance graph (balance `sim-graph20-spec.md`, `squad.md`). None of
them is in the game yet; only `Vampirism` among the traits is.

- **`Dash`** (G1, `chase`): an enemy with speed ≥ 2 whose chase move ends next to the slime
  bites in the same turn.
- **`SmellsBlood`** (G2, `can_see`): it sees 2 cells farther while the slime's edge unit is
  Blood.
- **`Alarm { radius }`** (G4, `alarm`): on the turn it first gets aggroed and sees the slime,
  every living calm enemy within `radius` of it (Chebyshev, through walls) gets aggroed and
  chases the slime's cell. The hunter is never woken (it is always aggroed anyway).
- **`Infects`** (G5, `enemy_melee`): after its bite, the slime's edge-most Blood becomes Pus.
- **Pus** (`fester`): a liquid with power 0 and no effect. After the slime's actions each round
  in which an enemy is engaged (like G3: no clock outside a fight), every Pus unit turns its Blood neighbours in body order into Pus (from the units as they were
  before the spread); any other unit between them stops it. Every policy spits a Pus edge
  off (1 action, a hit with power 0) instead of striking. `arranger` does not swap the kept
  liquid down onto Pus or next to it. No build is made of Pus.
- **`Nets`** (`enemy_melee`, `move_slime`): its bite also nets the slime for its next turn: no
  step (a step wastes the action), strikes allowed. `kite` strikes instead of stepping back.
- **Own vision** (`[[enemies]] vision`): the enemy's sight range; otherwise the level's.
- **G3 `rules.regeneration_in_fight_only`**: the slime's Regeneration grows Blood only while an
  enemy is engaged with it (aggroed and able to come, or adjacent). `false` models the game.

## Where the docs and the code disagree

Found while building the model (game `dev` at 44018ffe, 2026-11-25). The simulator follows the code.

1. `docs/creatures.md` lists Rat, Cat, Dog, Scorpion and not the live roster: in code
   `CreatureCatalog.IsRetired` retires those four, and the live enemies are Bat plus Mushroom,
   Undine, Cactus, Mandragora, Eye (5 HP, 0 armor, speed 1, 1 Water, no traits).
2. `docs/behavior.md` names only Dog as Aggressive; the five placeholders are Aggressive by the
   `Creature.Personality` default, which no doc mentions.
3. Status stacking: the docs describe Burning, Poisoned, Corroding as one 3-turn effect and
   Flammable as ×2; in code every strike adds another copy (`StatusQueue.Add` never merges),
   so damage over time stacks and Flammable multiplies (×4, ×8).
4. Mercury: `substances.md` says its dominance makes the slime Invisible, but no doc says what
   that does in a fight: non-aggroed enemies cannot see, chase or bite it (`IsDetectedBy`), so
   a pure Mercury slime always gets the first strike.
5. Blood vs Vampirism: a Blood strike deals 1 then heals the bat 2 (`Combat.ResolveHit` →
   `Substance.Apply` → `Vampirism`), and a Blood-dominant slime's Retaliation heals it too.
   A bat cannot be killed with Blood; no doc states this consequence.
6. `status-effects.md` and `strike-vs-puddle.md` have no row for Mercury; `substances.md` does.
7. `design-guard.md` and `turn-structure.md` describe A* paths; the A* graph has octile costs
   and its own tie-breaking, which the docs do not specify (see Board and movement).
