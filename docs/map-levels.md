# Map levels

A level as a map: a chain of rooms the slime walks through to the exit. Enemies roam in their
rooms, and walls block sight. It answers how many enemies engage the slime at once, whether the
slime reaches the exit, and what each fight nets. Size, doors and vision are the balance levers.
The spec was agreed with the balance session. Code: `src/map.rs` (generator, level run),
`src/fight.rs` (`Board`, `MapState`, episodes, the `strafe` and `lure` policies). Output: `[map-levels]` and
`[map-level-ends]` (`docs/report-format.md`).

## Input: a `[[levels]]` entry with `map`

`id`, `capacity`, `start` (default: full Water) and `action_points` work as for stage levels
([level-economy.md](level-economy.md)). `policy` is `strafe`, `lure` or `kite`. A level has either `stages` or
`map`, never both.

| `map` key | Meaning |
| --- | --- |
| `rooms` | Number of rooms, at least 1 |
| `room_side` | `[min, max]` inner width and height of a room, each random; min at least 3 |
| `enemies_per_room` | `[min, max]` enemies in each room, random |
| `enemy_kinds` | Each enemy's kind is a random pick from this list |
| `corridor` | Corridor cells between the two doorways of neighbouring rooms (0 or more) |
| `vision` | Enemy sight range (Chebyshev), blocked by walls |
| `blockers` | Optional: kinds placed in passages (doorways and corridor cells) instead of rooms, a random pick each |
| `blockers_per_level` | How many blockers; goes together with `blockers` |
| `river`, `river_liquids` | Optional: units the slime can drink at the exit, and their liquids in drinking order (cycling); go together |
| `drips` | Optional `{ cells, units }`: Water drip cells in passages, `units` each |
| `surface_rooms`, `sun_every` | Optional: the middle rooms under the sky, and how often the sun dries there (slime rounds; 1 day, 2 night); go together |
| `hunter` | Optional `{ kind, enter_round, scent_every = 1 }`: the G17 hunter |
| `camp` | Optional: kinds camped in the second-to-last room (the squad), one enemy each |
| — | A level may also carry `ladder = { level, difficulty, branch }` ([ladder.md](ladder.md)) |

## The map (`map::generate`, deterministic per seed)

- **Rooms** run west to east and all have their top row at y = 1. Everything else is wall.
- **Neighbouring rooms** are joined by a straight 1-cell passage: a doorway in each room's
  wall plus `corridor` cells. It runs on a random row that both rooms have.
- **The slime** starts at the middle of room 1's west side. **The exit** is the middle of the
  last room's east side.
- **Enemies** stand on random free cells of their room, farther than `vision` from the start,
  so nobody starts aggroed. A room with no such cell gets fewer enemies.
- **Blockers** stand on random open cells outside every room (doorways, corridors). They have
  no home room. A corridor is 1 cell wide, so a blocker on a corridor cell closes it until it
  dies; one in a doorway may leave a way around through the room.
- **Drips** stand on random passage cells free of blockers. Every passage lies on the only way
  between two rooms, so the slime crosses each drip on its walk.
- **The camp** stands on random cells of the second-to-last room, which lies on the way to the
  exit. Its home room is that room.
- **Surface rooms** are the `surface_rooms` rooms in the middle of the chain (from index
  `(rooms − surface_rooms) / 2`).
- `--trace-map LEVEL SEED` prints the map (`#` wall, `S` start, `X` exit, `~` drip, an enemy by
  its first letter) and the turn-by-turn log of that seed.

## Rules on top of the fight model

- **Sight.** An enemy sees the slime within `vision` (Chebyshev) when no wall lies on the
  Bresenham line strictly between them (`Board::line_of_sight`). Mercury invisibility still applies.
- **Movement.** Walls block movement and paths. An enemy inside its own room wanders only
  inside it; outside it (fled, or aggro lost) it wanders freely. Once aggroed it chases
  anywhere, through doorways, with the usual pursuit memory.
- **Round limit.** `rules.max_rounds × rooms`; beyond it the run is a `timeout` loss.
- **`strafe` policy:**
  1. If an aggroed enemy is adjacent, strike it with the edge unit (or wait on the
     last unit).
  2. Otherwise take one step along the path to the exit.
  3. If no path is free, strike any adjacent enemy to clear the way.

  It never hunts: a passive enemy it never touches stays behind.
- **A blocked way (every policy):** when no free path to the goal exists, the slime follows the
  path through creatures. If the next cell holds an enemy, or an aggroed enemy stands next to
  the slime, it hits it. Otherwise it steps on toward the blocker.
- **Fights only with enemies that can come:** an enemy holds the slime in an episode (and in
  lure's and kite's holding) when it is aggroed and either moves (speed above 0) or stands next
  to the slime. A stationary blocker that merely saw the slime does not.
- **`lure` policy.** It fights in corridors, so enemies reach it one at a time:
  1. If an aggroed enemy is adjacent, strike it (or wait on the last unit).
  2. While any enemy is aggroed, go to the fighting spot and wait there. The spot is the
     nearest open cell outside the rooms. Corridor cells with at most two open neighbours are
     preferred over doorways (a doorway opens onto three room cells). Ties go west.
  3. When calm, advance toward the exit. Standing outside a room, it first checks the next
     step: if two or more calm Aggressive enemies would see the slime there (range and line of
     sight), it waits. It waits at most `policies.lure_patience` actions per approach (the count
     resets when it enters a room or a fight starts), then goes.
  4. When the way is blocked, strike whatever stands next to it.
- **`kite` policy.** It walks and holds the fighting spot like `lure`. With an aggroed enemy
  next to it, it strikes and steps away like the matrix kite (`docs/rules-model.md`).
- **Episodes (fights).**
  - An episode opens when some living enemy is aggroed on the slime. It includes the action
    that caused it, such as a first strike.
  - It closes when no living enemy is aggroed (they died), or at the end of the run.
  - It records the most aggroed enemies adjacent to the slime at once, checked after each
    slime action and each enemy turn.
  - When it closes, a living slime eats the corpses of the enemies that died in it, up to
    capacity. There is no walking and no time.
  - `net` = corpse units + grown − spent − bitten − status − thorns during the episode, ignoring
    capacity.
- **Drinking (every policy).** When no aggroed enemy is adjacent, the slime standing on a drip
  cell or on the exit drinks 1 unit per action until it is full or the source is empty. Drunk
  units go onto the edge. The run ends on the exit once the river is empty or the slime is
  full. Drunk units are not part of an episode's `net`.
- **Sun.** After the slime's actions of round `r`, when `r` divides by `sun_every` and the
  slime stands in a surface room, the edge-most Water unit dries. Taking the last unit kills
  (cause `sun`). Sun losses are not part of an episode's `net`.
- **The hunter (G17).** It enters at the start of the enemy phase of round `enter_round`, or
  earlier once the slime has entered the second-to-last room, on the exit or the free open
  cell nearest to it. It enters aggroed. Every `scent_every` rounds it learns the slime's cell
  (through walls) and chases it by the shortest path; between scents it follows its pursuit
  memory. Alarms never wake it. An episode it takes part in is a hunter episode. Its outcome
  per seed: `killed_slime` when the slime died to its bite, else `killed` when it died, else
  `alive` (also when it never entered).
- **Min body:** the body at the start and after each closed episode; 0 when the slime dies.
- **Engagement checks** run after every slime action, every enemy turn and every pulse of the
  slime's statuses.

## Limits

- `lure` assumes aggro is sticky (Aggressive and Passive enemies). A Cowardly enemy that drops
  aggro every turn could make it walk back and forth between the spot and the exit until the
  round limit.
- `lure` judges watchers by enemy personality: Passive enemies (bats) never count, because
  they do not engage until hit.
- An enemy killed by a single strike outside an episode opens no episode, so its corpse and
  the spent unit are not counted. Today that cannot happen: no live enemy dies to one strike.
- An enemy standing on the exit blocks it; the slime waits (or strikes it when adjacent).
- No fountains or puddles on map levels; drink sources are only the river at the exit and drips.
- One corridor shape: straight, 1 cell wide.
