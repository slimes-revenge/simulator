# Map levels

A level as a map: a chain of rooms the slime walks through to the exit. Enemies roam in their
rooms, and walls block sight. It answers how many enemies engage the slime at once, whether the
slime reaches the exit, and what each fight nets. Size, doors and vision are the balance levers.
The spec was agreed with the balance session. Code: `src/map.rs` (generator, level run),
`src/fight.rs` (`Board`, `MapState`, episodes, the `strafe` policy). Output: `[map-levels]` and
`[map-level-ends]` (`docs/report-format.md`).

## Input: a `[[levels]]` entry with `map`

`id`, `capacity`, `start` (default: full Water) and `action_points` work as for stage levels
([level-economy.md](level-economy.md)). `policy = "strafe"`. A level has either `stages` or
`map`, never both.

| `map` key | Meaning |
| --- | --- |
| `rooms` | Number of rooms, at least 1 |
| `room_side` | `[min, max]` inner width and height of a room, each random; min at least 3 |
| `enemies_per_room` | `[min, max]` enemies in each room, random |
| `enemy_kinds` | Each enemy's kind is a random pick from this list |
| `corridor` | Corridor cells between the two doorways of neighbouring rooms (0 or more) |
| `vision` | Enemy sight range (Chebyshev), blocked by walls |

## The map (`map::generate`, deterministic per seed)

- **Rooms** run west to east and all have their top row at y = 1. Everything else is wall.
- **Neighbouring rooms** are joined by a straight 1-cell passage: a doorway in each room's
  wall plus `corridor` cells. It runs on a random row that both rooms have.
- **The slime** starts at the middle of room 1's west side. **The exit** is the middle of the
  last room's east side.
- **Enemies** stand on random free cells of their room, farther than `vision` from the start,
  so nobody starts aggroed. A room with no such cell gets fewer enemies.
- `--trace-map LEVEL SEED` prints the map (`#` wall, `S` start, `X` exit, an enemy by its first
  letter) and the turn-by-turn log of that seed.

## Rules on top of the fight model

- **Sight.** An enemy sees the slime within `vision` (Chebyshev) when no wall lies on the
  Bresenham line strictly between them (`Board::line_of_sight`). Mercury invisibility still applies.
- **Movement.** Walls block movement and paths. An enemy inside its own room wanders only
  inside it; outside it (fled, or aggro lost) it wanders freely. Once aggroed it chases
  anywhere, through doorways, with the usual pursuit memory.
- **Round limit.** `rules.max_rounds × rooms`; beyond it the run is a `timeout` loss.
- **`strafe` policy:**
  1. If an aggroed enemy is adjacent, strike it with the liquid held most (or wait on the
     last unit).
  2. Otherwise take one step along the path to the exit.
  3. If no path is free, strike any adjacent enemy to clear the way.

  It never hunts: a passive enemy it never touches stays behind.
- **Episodes (fights).**
  - An episode opens when some living enemy is aggroed on the slime. It includes the action
    that caused it, such as a first strike.
  - It closes when no living enemy is aggroed (they died), or at the end of the run.
  - It records the most aggroed enemies adjacent to the slime at once, checked after each
    slime action and each enemy turn.
  - When it closes, a living slime eats the corpses of the enemies that died in it, up to
    capacity. There is no walking and no time.
  - `net` = corpse units + grown − spent − bitten − status during the episode, ignoring
    capacity.
- **Min body:** the body at the start and after each closed episode; 0 when the slime dies.
- **Engagement checks** run after every slime action, every enemy turn and every pulse of the
  slime's statuses.

## Limits

- Only the `strafe` policy exists; `lure` comes next.
- An enemy killed by a single strike outside an episode opens no episode, so its corpse and
  the spent unit are not counted. Today that cannot happen: no live enemy dies to one strike.
- An enemy standing on the exit blocks it; `strafe` waits (or strikes it when adjacent).
- No fountains, puddles or spit on map levels yet.
- One corridor shape: straight, 1 cell wide.
