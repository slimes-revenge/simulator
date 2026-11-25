# Level economy

The body budget of a level: what the slime gains from corpses and fountains against what it
spends and loses in fights, stage by stage, with the body carried over. It answers whether a
player reaches the end of a level and with what net gain per fight. The design targets are in
the balance work: Easy +1…+2, Normal +1, Hard 0…+1 units per typical fight. The spec was agreed
with the balance session. Code: `src/level.rs`. Output: `[levels]` and `[level-ends]` in
`design-guard.txt` (`docs/report-format.md`).

## Input: `[[levels]]` in the design file

| Key | Meaning |
| --- | --- |
| `id` | Stable, unique id of 1–14 characters, printed in the report |
| `capacity` | Body capacity for this level. It also bounds Regeneration inside the level's fights |
| `start` | The body at the start, in volume order, 1..=`capacity` units. Default: `capacity` × Water |
| `action_points` | Slime actions per round in every fight of the level |
| `policy` | `face-tank` only. Any other value is a design-file error |
| `stages` | 1 to 999 stages, in order: `{ kind = "fight", enemy, enemies }` or `{ kind = "fountain", liquid, units }` |

`data/design.toml` holds two placeholder levels (`sample-ap1`, `sample-ap2`) until balance
defines real ones. Experiments use their own file through `--design`.

## Rules

- **Fight stage:** one fight from the matrix layout. The slime starts at `matrix.slime_cell`
  and the enemies stand at `matrix.enemy_rows`. The slime brings its current body, in order.
  The seed is `seed × 1000 + k`, where `k` is the 0-based stage index (the report numbers
  stages from 1), with `matrix.seeds` seeds per level.
- **After a won fight** the slime eats every corpse of that fight. A corpse is the enemy's
  `volume` list. Units are appended in order while the body is below `capacity`; the rest is
  `wasted`. Digesting time is ignored: between fights no enemy acts.
- **A lost fight** ends the run. The stop is counted as `stage<k>:<cause>`. After a death the
  body is 0; after a `timeout` it is what the slime still had, so the ledger holds either way.
- **Fountain stage:** the slime drinks `units` of `liquid` up to `capacity`. What it does not
  drink is `wasted` and lost: the fountain keeps nothing. Drinking costs no fight time.

## Ledger per stage

`after = before − spent − bitten − status + grown + eaten + drunk`, where `grown` is the Blood
added by Regeneration during the fight (counted on each pulse, not derived from the others).
`net = eaten + drunk + grown − spent − bitten − status`, ignoring capacity. Overflow shows in
`wasted`; it is the budget available for swapping liquids.

Every average is over the seeds that **reached** the stage. A stage nobody reached shows
`reached 0`, `win -` and zeros.

## Not modelled yet

- Kite and other policies.
- Digesting time.
- Fountains that keep their rest.
- Choosing which corpse units to eat.
- Levels with room shapes.
