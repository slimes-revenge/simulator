# Report formats

The simulator writes the same results in two files:

- **`design-guard.txt`** — plain text for the balance session: no timestamps, stable order,
  fixed columns, so `diff old new` shows exactly what shifted. Agreed with the balance session.
- **`design-guard.json`** — the contract with the site (`slimes-revenge/site`), which validates
  it with zod and draws it.

Both are rebuilt by `cargo run --release` and published on every push to `dev` as assets of
the rolling `latest` release:

- https://github.com/slimes-revenge/simulator/releases/download/latest/design-guard.txt
- https://github.com/slimes-revenge/simulator/releases/download/latest/design-guard.json

The text report is also committed as `reports/design-guard.txt`. CI fails when it is stale, so
the diff of every pull request that changes rules or numbers shows how the matrix moved.

## `design-guard.txt`

```
# Slime's Revenge design guard — plain-text report, format 1

[design]
rules.board_width = 10
...
liquids.Lava.power = 1
enemies.Bat.traits = Vampirism
matrix.enemy_rows = 5 4 6

[cells]
ap policy     build      encounter        win  rounds killed   left  spent bitten status  cause
1  face-tank  lava       mushroom-1       100    4.00   1.00   7.00   2.00   1.00   0.00  -
1  face-tank  start      bat-3              0  176.82   0.88   0.88   2.92   2.20   0.00  timeout=44 melee:Bat=6
```

- **Header line:** format version. No time, commit or seed list. The release notes and the
  JSON carry the commit.
- **`[design]`:** every value of the design file the run used, one `key = value` per line, in
  file order. An empty list prints as `key = ` with nothing after it.
  - Tables flatten to dotted keys. Arrays of tables are keyed by their `name`
    (`liquids.Lava.power`, `enemies.Bat.hit_points`). Lists are joined by spaces.
  - When `--design` was given, the first line is `design.file = <path as given>`.
- **`[cells]`:** one header line, then one line per cell.
  - Sorted by `ap` (ascending), `policy`, then build in design order (`start`, then
    `[[liquids]]` order), then encounter in design order (roster order, then count). Build and
    encounter order are not alphabetical.
  - Fixed-width columns, separated by spaces.
  - When difficulty levels arrive, a `difficulty` column goes between `policy` and `build`,
    and that raises the format number.

| Column | Meaning |
| --- | --- |
| `ap` | Slime actions per round |
| `policy` | How the slime plays (`face-tank`) |
| `build`, `encounter` | Ids, the same as in the JSON |
| `win` | Win rate in percent, an integer (exact with 50 seeds) |
| `rounds` | Average rounds until the fight ended |
| `killed` | Average enemies killed (progress inside a loss) |
| `left` | Average units left at the end |
| `spent` | Average units spent on strikes |
| `bitten` | Average units lost to enemy bites |
| `status` | Average units lost to the slime's own status pulses |
| `cause` | `-` when every fight was won. Otherwise every loss cause as `cause=fights`, by count descending, then by name |

All averages are over every seed and printed with exactly 2 decimals. Body ledger: `spent + bitten
+ status + left` = the build's units, plus whatever Blood Regeneration grew.

### `[levels]` and `[level-ends]`

These appear only when the design file has `[[levels]]`. The rules are in [level-economy.md](level-economy.md).

```
[levels]
level          stage kind     detail         reached win before  spent bitten status  grown  eaten  drunk wasted  after     net
sample-ap2         1 fight    mushroom-1         100 100  10.00   5.00   2.00   0.00   0.00   1.00   0.00   0.00   4.00   -6.00
sample-ap2         3 fountain Water x3             0   -   0.00   0.00   0.00   0.00   0.00   0.00   0.00   0.00   0.00   +0.00

[level-ends]
level          end    min worst wasted  stops
sample-ap2       0   4.00     4   0.00  stage2:melee:Bat=50
```

`[levels]` has one line per stage, levels in file order and stages in order. Averages are over
the seeds that reached the stage, with 2 decimals; `net` carries a sign.

| Column | Meaning |
| --- | --- |
| `level`, `stage` | Level id, 1-based stage number |
| `kind`, `detail` | `fight` with `<enemy>-<count>`, or `fountain` with `<Liquid> x<units>` |
| `reached` | Percent of seeds that started this stage |
| `win` | Percent of those that won the fight; `-` for a fountain or a stage nobody reached |
| `before`, `after` | Body at the stage start and end (after eating or drinking; 0 after a loss) |
| `spent`, `bitten`, `status` | Units lost to strikes, bites, the slime's own status pulses |
| `grown` | Blood grown by Regeneration during the fight |
| `eaten`, `drunk` | Units gained from corpses and from the fountain |
| `wasted` | Corpse or fountain units that did not fit into capacity |
| `net` | `eaten + drunk + grown − spent − bitten − status`, ignoring capacity |

`[level-ends]` has one line per level:

| Column | Meaning |
| --- | --- |
| `end` | Percent of seeds that finished every stage |
| `min` | Average over seeds of the smallest body between stages (start included, losses excluded) |
| `worst` | The smallest body between stages in any seed |
| `wasted` | Average total wasted units per seed |
| `stops` | Where runs ended, `stage<k>:<cause>=seeds`, by count descending; `-` when all finished |

### CLI for experiments

```bash
cargo run --release -- --design my.toml --no-json --out-txt -     # text to stdout
cargo run --release -- --only-builds lava,water --only-enemies Mushroom,Bat --out-txt my.txt
cargo run --release -- --trace lava mushroom-1 0 1   # turn-by-turn log: build encounter seed ap
```

A filter naming an unknown build id or enemy name fails with exit code 2 and lists the valid
ones. `--trace` prints one fight's turn-by-turn log (actions, damage, statuses, volume) and no report.
The log is for checking hand calculations, not a format to diff.

## `design-guard.json`

Changing a field's name, type or meaning raises `schemaVersion`; adding a field does not.

**Where:** every push to `dev` publishes it at
`https://github.com/slimes-revenge/simulator/releases/download/latest/design-guard.json`.
Locally `cargo run --release` writes `design-guard.json` in the repository root.

## Shape

The file is a **JSON array of reports**, one per value of `matrix.action_points` in
`data/design.toml` (today `[1, 2]`: 1 action per round is the game today, 2 is the design).
Every report is complete on its own.

```json
[
  {
    "schemaVersion": 1,
    "generatedUtc": "2026-11-25T17:00:00Z",
    "commit": "<git sha the report was built from>",
    "policy": "face-tank",
    "actionsPerRound": 1,
    "seeds": 50,
    "builds": [
      { "id": "start", "name": "Starting mix", "units": { "Water": 2, "Oil": 1, "Poison": 1, "Acid": 1, "Lava": 1 } },
      { "id": "lava", "name": "10 Lava", "units": { "Lava": 10 } }
    ],
    "encounters": [
      { "id": "mushroom-2", "enemy": "Mushroom", "enemies": 2 }
    ],
    "cells": [
      {
        "build": "lava",
        "encounter": "mushroom-2",
        "enemy": "Mushroom",
        "enemies": 2,
        "winRate": 1.0,
        "avgRounds": 5.0,
        "avgUnitsLeft": 4.0,
        "avgUnitsSpent": 3.0,
        "avgUnitsBitten": 3.0,
        "avgUnitsStatus": 0.0,
        "avgEnemiesKilled": 2.0,
        "cause": null,
        "causes": {}
      }
    ]
  }
]
```

## Fields

| Field | Type | Meaning |
| --- | --- | --- |
| `schemaVersion` | int | `1` |
| `generatedUtc` | string | When the report was built, `YYYY-MM-DDTHH:MM:SSZ` |
| `commit` | string | The simulator commit (`GITHUB_SHA` in CI, else `git rev-parse HEAD`, else `unknown`) |
| `policy` | string | How the simulated slime plays; `face-tank` only in v1 |
| `actionsPerRound` | int | Slime actions per round; enemies always act once |
| `seeds` | int | Fights per cell; seeds `0 .. seeds-1` |
| `builds[]` | object | `id` (stable key), `name` (label), `units` (liquid → count). Order: `start`, then one pure build per liquid in `[[liquids]]` order |
| `encounters[]` | object | `id` = `<enemy lowercase>-<count>`, `enemy`, `enemies` (count). Order: roster order, then count |
| `cells[]` | object | One per build × encounter, builds outer, encounters inner |

Cell fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `build`, `encounter` | string | Ids from `builds[]` and `encounters[]` |
| `enemy`, `enemies` | string, int | Copied from the encounter, so a cell reads on its own |
| `winRate` | number 0..1 | Share of fights the slime won (every enemy dead) |
| `avgRounds` | number | Rounds until the fight ended, over all fights (won or lost) |
| `avgUnitsLeft` | number | Units in the slime's volume at the end, over all fights (a loss leaves 0, a timeout leaves what it had) |
| `avgUnitsSpent` | number | Units the slime spent on strikes, over all fights |
| `avgUnitsBitten` | number | Units lost to enemy bites, over all fights |
| `avgUnitsStatus` | number | Units lost to the slime's own status pulses, over all fights |
| `avgEnemiesKilled` | number | Enemies killed, over all fights |
| `cause` | string or null | The most common cause of a loss; `null` when every fight was won |
| `causes` | object | Every loss cause → number of fights it ended (`{}` when all were won) |

Averages are rounded to 2 decimals.

Loss causes:

| Cause | Meaning |
| --- | --- |
| `melee:<Enemy>` | That enemy's bite took the slime's last unit |
| `status:<Status>` | A status pulse on the slime took its last unit (not possible in v1: enemies apply no statuses) |
| `timeout` | Nobody won within `rules.max_rounds` rounds: a stalemate, for example a Blood slime against a Vampirism bat that heals more than it loses, or a slime down to its last unit next to a Passive enemy that was never hit |
