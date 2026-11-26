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
  mix (Water, Water, Water, Poison, Acid, Lava from the centre out; the game's third unit is Oil, which Oleg removed) has Lava on the edge.
- Retaliation: when the slime has a dominant liquid before the hit and survives it, the
  attacker receives that liquid's effect. Under `rules.retaliation_pulses` > 0 its statuses last
  that many pulses (the design: 3, a full status like a strike; the fallback 2 lasts through
  the enemy's next turn, since statuses pulse at the start of their owner's turn); 0 keeps the liquid's usual duration (the game).
- Dominance (affinity): one liquid with at least `rules.dominance_percent` of the basis.
  `rules.dominance_basis = "current"` (the body now: the design's 60 %, the game's 80 %) or
  `"capacity"` (the fallback: a share of the capacity, so a bitten body does not fall into an
  affinity).
  Water: blocks and removes Burning. Blood: Regeneration
  (`statuses.regeneration_growth` Blood per pulse up to capacity, queued first, removed when
  dominance changes). Mercury: by `rules.mercury_affinity` — `defence` (the design, D7 below)
  or `invisible` (the game, the fallback): an enemy that is not aggroed cannot see, chase or
  bite the slime.

## Liquid effects (`Substance.Apply`, on strike and retaliation)

| Liquid | Effect |
| --- | --- |
| Water | Wet (`statuses.wet`) |
| Poison | Poisoned: `statuses.poisoned_pulse` HP per pulse |
| Acid | Corroding: strips `corrosion` armor per pulse, else `statuses.corroding_pulse` HP |
| Lava | Burning: `statuses.burning_pulse` HP per pulse × Flammable |
| Blood | Nothing; a Vampirism creature heals `statuses.vampirism_heal` |
| Mercury | Nothing (power 2 in the design; the game's is 1) |
| Vitriol | Corroding that strips at least 1 armor per pulse |
| Alkali | Instability for its `pulses` |
| Sulfur | Flammable (`statuses.flammable`) |
| Ambrosia | Heals a Living target 1 instead of hurting it (R2) |
| Pus, Phosphorus, Aquavita, Fat, Resin, Aphrodisiac | Nothing yet: a hit for the liquid's `power`; their rows come from the balance session |

**Heavy liquids** (`rules.heavy`, design): each listed liquid the slime holds at least its count
of (Lava 3, Mercury 2) takes 1 action point off every round, checked at the round's start. The
points never drop below `rules.heavy_min_action_points` (2), or below the base when that is
lower.

The matrix builds a pure body only of `matrix.build_liquids`.

## Statuses (`StatusQueue`)

- FIFO queue, each status pulses once at the start of its owner's turn, then loses one turn;
  it expires at 0. Damage over time ignores armor.
- **Statuses stack.** `StatusQueue.Add` does not merge kinds: three Lava strikes are three
  Burnings, each pulsing. Each Flammable doubles again (two Sulfur strikes: ×4).
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
- **`Alarm { radius }`** (G4, `alarm`): after its own turn, the first time it is aggroed and
  sees the slime (also when the slime struck it first), every living calm enemy within
  `radius` of it (Chebyshev, through walls) gets aggroed and chases the slime's cell. Once per
  enemy; a woken enemy with an alarm raises its own when it sees the slime. The hunter is
  never woken.
- **`Infects`** (G5, `enemy_melee`): after its bite, the slime's edge-most Blood becomes Pus.
- **Pus** (`fester`): a liquid with power 0 and no effect. After the slime's actions each round
  in which an enemy is engaged (like G3: no clock outside a fight), every Pus unit turns its Blood neighbours in body order into Pus (from the units as they were
  before the spread); any other unit between them stops it. Every policy spits a Pus edge
  off (1 action, a hit with power 0) instead of striking. `arranger` does not swap the kept
  liquid down onto Pus or next to it. No build is made of Pus. A body of one Pus unit under
  `last_drop_strike` spits it every action without spending it: the slime can no longer hurt
  anyone, and the fight ends in a timeout or its death.
- **`Nets`** (`enemy_melee`, `move_slime`): its bite also nets the slime for its next turn: no
  step (a step wastes the action), strikes allowed. `kite` strikes instead of stepping back.
- **Own vision** (`[[enemies]] vision`): the enemy's sight range; otherwise the level's.
- **G3 `rules.regeneration_in_fight_only`**: the slime's Regeneration grows Blood only while an
  enemy is engaged with it (aggroed and able to come, or adjacent). `false` models the game.

## R1: reactions in the body (`react`, `[[reactions]]`, `[elements]`)

Agreed design rules (balance `sim-reactions-spec.md`); not in the game yet.

- **When:** after the slime's actions each round, before Pus spreads, the sun and the enemies.
- **Chain:** the reacting pair of neighbours nearest the edge (either order) becomes its
  `makes` in place; the body closes up; repeat until no neighbours react. Water + Lava and
  Lava + Aquavita vanish, Poison + Blood become 2 Pus, Alkali + Acid become 1 Water, Resin +
  Lava leave the Lava (the chain goes on from it).
- **Death:** a chain that empties the body kills (cause `reaction`). With
  `rules.reaction_can_kill = false` a pair that would empty it does not react.
- **Stone:** a chain drops one blocker per `stone` pair that reacted in it (Water + Lava: a
  stone; Resin + Lava: amber), each on the free open
  neighbour of the slime nearest the nearest living enemy (ties: row, then column), never on
  the exit or a drip. A stone
  blocks walking like a creature and crumbles after `rules.stone_rounds` slime rounds.
- **Losses:** units a reaction takes count as lost in an episode's `net` and in `[economy]`
  `net`.
- **At the exit:** when a map run ends, units that met in its last turn or through eaten
  corpses react once more before the body leaves the level (`body_out`, the next ladder level).
  A reaction that empties the body there is a loss (cause `reaction`).
- **Encouragement:** once per slime turn, a strike or spit with the edge unit hits
  `elements.encouragement_bonus` harder when the unit under it encourages it (the next liquid
  in an `elements.circles` circle: Water → Phosphorus → Lava → Mercury → Aquavita → Water;
  Vitriol → Alkali → Ambrosia → Acid → Poison → Vitriol).

## R2: elements in combat (`[elements]`, `hit`, `enemy_melee`)

Agreed design rules (balance `sim-reactions-spec.md`, `bestiary-numbers.md`); not in the game.

- **Elements.** The 10 circle liquids are their own element; the other 6 have none. An enemy's
  element is its `element`, else its corpse's most common liquid (the first on a tie), as its
  own element or through `elements.corpse` (Blood, Fat → Alkali; Pus → Poison;
  Sulfur → Acid; Resin → Ambrosia). A ⊣ B (A suppresses B): B is two places after A in a
  circle (Water ⊣ Lava ⊣ Aquavita ⊣ Phosphorus ⊣ Mercury ⊣ Water; Vitriol ⊣ Ambrosia ⊣ Poison ⊣
  Alkali ⊣ Acid ⊣ Vitriol).
- **The slime's hit** (strike or spit; its element is the edge unit's):
  - dominates the target: +1 damage (`suppress = "plus1"`; `"double"`: ×2), counted `elem_plus`;
  - suppressed by the target: plain damage and no encouragement bonus;
  - same element: no damage, no status, the unit is kept (the target is aggroed).
- **An enemy's bite** (its element against the slime's edge unit's):
  - dominates: +1 (or ×2);
  - suppressed: it lands `suppressed_hit_percent` (25) of the time; a miss costs the enemy
    `miss_self_damage` (1) HP, counted `elem_miss`;
  - same element: no bite.
- **Status immunity** (`status_immune`): a status does not take on enemies of the listed
  elements; the hit still hurts. An enemy's own `status_immune` list adds to its element's (the
  weeping ghost: Instability). `status_double`: it takes twice (two copies) instead.
- **Ambrosia** heals a Living target (`elements.living` = Alkali) by 1 instead of hurting it.
- **Strike rows of the new liquids** (balance `liquids.md`, final list): Vitriol — Corroding
  (at least 1 armor per pulse; the strike strips none); Alkali — Instability for its `pulses`
  (2); Sulfur — Flammable. Liquids whose status is not modelled yet hit with no status.

## R3: policies and reactions (`reader`, careful drinking)

- **`reader`** = `kite`, and before a strike, when the edge does not dominate the target and a
  unit 1 or 2 places under it does, it moves that unit to the edge (one action, counted as a
  swap) unless the move makes reacting neighbours; then it strikes. On maps it walks like
  `kite`.
- **Careful map policies** (`lure`, `kite`, `reader`; not `strafe`, the newcomer of maps): an
  edge unit that reacts with the unit under it is spat away (one action); a unit they would
  drink onto an edge it reacts with is poured away instead (one action, the source loses it).
- **`arranger`** skips a swap that would make reacting neighbours.
- `newcomer` and `strafe` do none of this: their reactions are the measured mistakes.
- **No repeated useless strikes (R2 stalls).** When the edge is the target's own element,
  every policy brings up a unit 1 or 2 under the edge that is not (one action), unless that
  makes reacting neighbours; `newcomer` and `strafe` first waste one strike. With nothing to
  bring up the slime gives up on that enemy: it neither strikes nor waits for it (on maps it
  walks on to the exit past it). The waste and the giving up are per enemy. A moving enemy it
  cannot hurt that blocks the way is waited for; when stationary given-up enemies (not dying
  of their statuses) close every way to the goal, the run ends as a loss, cause `stalemate`. A matrix fight in which the slime gives up on every living enemy (none dying of
  its statuses) ends at once the same way. `lure` and `kite` hold for an enemy only while it
  has a way to the slime past walls, stones and given-up enemies. `arranger` never swaps the
  target's own element up. A bring-up never raises the number of reacting pairs.

## D1, D3: puddles, Pus burps, Infected (`[pus]`, balance `sim-3d-spec.md`)

Agreed design rules; not in the game.

- **Puddles** (`Fight::puddles`): one per cell, 1–2 units, for the whole fight. An enemy that
  steps into one (any move that ends there) gets the liquid's effect without damage, as from a
  strike (`Source::Puddle`, counted in the status damage); a Pus puddle gives Infected instead.
  On a sun tick (slime in a surface room) every Water puddle in a surface room loses 1 unit.
- **Pus as a bad affinity:** Pus at least `pus.affinity_percent` (60) of the body now. While
  it holds, every `pus.burp_every`-th (3rd) slime round is a burp: the slime takes no actions
  and `pus.burp_puddles` (2) new 1-unit Pus puddles fall on free neighbouring cells (empty
  cells first, then 1-unit Pus puddles; row-major), from nothing, in or out of a fight.
- **Infected** (Living enemies only, `elements.living`): `pus.infected_damage` HP per pulse
  and a bite `pus.infected_bite` weaker (a bite of 0 does nothing), for
  `pus.infected_pulses`; it refreshes, never stacks. From Pus puddles always; from a Pus hit
  (strike or spit) only on levels with `pus_infects = true`.
- **`pus` policy** (D5): `kite` until Pus is a bad affinity (matrix: always `kite`). Then on
  maps:
  1. a non-Living neighbour, or any time once the hunter is in (Hard): `kite`, which never
     strikes the Living with Pus before `pus_infects` levels;
  2. a Living enemy next to it: strike it on `pus_infects` levels (or with the last drop);
     else step back out of its reach (so it comes again across the puddles the burps laid),
     or stand when it cannot;
  3. a Living enemy coming: hold among the puddles;
  4. calm and not full: gather its Pus puddles (stand on one: 1 unit per action; or step onto
     a neighbouring one), else wait for a burp — so it reaches the exit full;
  5. otherwise walk on like `kite`; it drinks a drip or the river only while Pus stays a bad
     affinity, and at the exit it leaves the rest of the river once a drink would end it.
- Infected lasts `infected_pulses` (4) pulses; the last one ends before that turn's bite, so it
  weakens 3 bites. A Dash bite on arrival lands before the puddle infects (the step-in follows
  the whole move); a fast walk infects only on the cell it ends on.
- **Dross:** dead cargo a stop leaves (no power, no effect, a cell). Bites take it like any
  unit, but it is not life: a body of nothing but Dross is dead.

## D7: the Mercury affinity and the `mercury` policy (balance `sim-3d-spec.md`)

Agreed design rules; not in the game.

- **Defence** (`rules.mercury_affinity = "defence"`): a bite on a slime whose affinity is
  Mercury deals half its power, rounded up (after the element and Infected changes), and each
  unit it takes is the edge-most non-Mercury unit while one is left, then the edge. The slime is
  seen like any other. "The slime cannot be moved" has nothing to act on yet: no ram, current,
  whirlpool or pull exists in the model.
- **`mercury` policy** (matrix and maps):
  1. without the affinity: `kite` (careful drinking on maps);
  2. with it: it goes for the nearest living enemy it sees (sight range, walls block) that it
     can reach and hurt, and strikes it when next to it, instead of kiting or waiting;
  3. before a strike (one action, like `reader`): it brings Mercury from 1 or 2 under the edge
     up when spending one keeps the affinity and the target is not of Mercury's element, else
     a non-Mercury unit; the stall rule brings a non-Mercury unit up against Mercury enemies;
  4. while Mercury is its affinity it keeps the body within Mercury × 100 / `dominance_percent`: it
     drinks no non-Mercury unit past that (at the exit it leaves the rest of the river), eats
     a corpse's Mercury first and leaves what would thin it, and a stop refills it only up to
     that (`ladder::at_stop`).
- At 1 action a round the brought-up unit is bitten before it strikes (the defence takes
  non-Mercury first), so against a Mercury enemy the slime strikes only at 2 actions or more.

## D8: build policies (`b-<liquid>`, `b-auto`) and the Poisoned corpse (balance `sim-3d-spec.md`, `builds.md`)

Agreed design rules; not in the game.

- **`b-<liquid>`** (any liquid but Dross, lower case: `b-lava`, `b-acid`, …; matrix and maps):
  `kite` (careful drinking on maps). While the liquid is its affinity:
  1. before a strike on a target the liquid dominates (R2), it brings a unit of it from 1 or 2
     under the edge up (one action, like `reader`); against a target of its own element the
     stall rule brings another unit up or gives the target up;
  2. it keeps the share: it drinks nothing that would take the body past the liquid's
     count × 100 / `dominance_percent` (at the exit it leaves the rest of the river), and eats
     a corpse's units of the liquid first, leaving what would thin it.
  Without the affinity it is plain `kite`. Stops refill it like any other row.
- **`b-auto`**: each turn plays `b-<liquid>` for the liquid of its affinity, or, with none,
  its most common liquid (the first in the `Liquid` order on a tie), never Dross; it eats every
  corpse.
- `mercury` shares the share rule: one cap, `build_full`.
- Puddles of the liquid are not gathered: only Pus burps make puddles today.
- **The Poisoned corpse** (map runs, every policy): a Living enemy (`elements.living`) that
  dies with a Poisoned status leaves half its corpse (rounded up, the first units) as Pus and the
  rest as its own liquid. The dead keep the status that killed them, so a killing Poisoned pulse
  counts. Stage levels (`stages`) eat the enemy's listed corpse and do not apply it.

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
8. Oil and Tar: Oleg removed them from the design (2026-11-26, 17 liquids); the game still has
   Oil (the starting mix, Instability + Flammable, the Oil dominance's Forever Flammable). The
   model follows the design here: no Oil, and the starting mix holds Water in its place.
