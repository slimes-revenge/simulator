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
# Slime's Revenge design guard — plain-text report, format 2

[design]
rules.board_width = 10
...
liquids.Lava.power = 1
enemies.Bat.traits = Vampirism
matrix.enemy_rows = 5 4 6

[cells]
ap policy     build      encounter        win  rounds killed   left  spent bitten status  cause
1  newcomer   lava       mushroom-1       100    4.00   1.00   7.00   2.00   1.00   0.00  -
1  newcomer   start      bat-3              0   23.96   1.08   0.08   3.00   2.92   0.00  melee:Bat=46 timeout=4
```

- **Header line:** format version. It goes up when an existing section's columns change.
  No time, commit or seed list. The release notes and the JSON carry the commit.
- **`[design]`:** every value of the design file the run used, one `key = value` per line, in
  file order. An empty list prints as `key = ` with nothing after it.
  - Tables flatten to dotted keys. Arrays of tables are keyed by their `name`, else their `id`,
    else their 1-based position (`liquids.Lava.power`, `levels.sample-ap1.stages.3.units`).
    Lists are joined by spaces.
  - When `--design` was given, the first line is `design.file = <path as given>`.
- **`[cells]`:** one header line, then one line per cell.
  - Sorted by `ap` (ascending), `policy` (in `matrix.policies` order), then build in design order (`start`, then
    `[[liquids]]` order), then encounter in design order (roster order, then count). Build and
    encounter order are not alphabetical.
  - Fixed-width columns, separated by spaces.
  - When difficulty levels arrive, a `difficulty` column goes between `policy` and `build`,
    and that raises the format number.

| Column | Meaning |
| --- | --- |
| `ap` | Slime actions per round |
| `policy` | How the slime plays: `newcomer` (walk to the nearest enemy, strike with the edge unit, never swap) or `arranger` (the same, plus at most one swap before a strike; `docs/rules-model.md`) |
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

### `[economy]` and `[damage]`

Balance slices, agreed with the balance session. One line per cell, with the same key columns
and order as `[cells]`: `ap policy build encounter`. All values are averages over all seeds,
with 2 decimals. A breakdown is written `Name:2.00,Name:1.00`, or `-` when empty. Liquids
follow their fixed order (Water, Oil, Poison, Acid, Blood, Lava, Mercury); statuses are
alphabetical. A column appears only once its mechanic exists in the model, so no column is a placeholder for a mechanic that is not built yet. Some columns can still be zero everywhere in today's data, for example `status` while no enemy applies statuses to the slime.

```
[economy]
ap policy     build      encounter         spent bitten status  grown  swaps     net  by
1  newcomer   start      bat-3              3.00   2.92   0.00   0.00   0.00   -5.92  spent=Water:0.08,Oil:0.92,Poison:1.00,Lava:1.00 bitten=Water:1.84,Oil:0.08,Acid:1.00 status=-

[damage]
ap policy     build      encounter        strike status  retal overkill doomed per_unit  status_by
2  newcomer   lava       eye-3              4.00   3.00   6.00     0.00   3.00     3.25  Burning:9.00
```

`[economy]`, the body ledger of one fight:

| Column | Meaning |
| --- | --- |
| `spent`, `bitten`, `status` | Units spent on strikes, popped by bites, lost to the slime's own status pulses (the same numbers as `[cells]`) |
| `grown` | Blood grown by Regeneration |
| `swaps` | Actions spent swapping two units of the body (`newcomer` never swaps) |
| `net` | `grown − spent − bitten − status` (eating and fountains are in `[levels]`) |
| `by` | `spent=` by liquid, `bitten=` by liquid popped, `status=` by status |

`[damage]`, the HP the slime took off enemies, by source:

| Column | Meaning |
| --- | --- |
| `strike` | HP removed by strikes |
| `status` | HP removed by pulses of statuses the slime's strikes applied |
| `retal` | HP removed by pulses of statuses Retaliation applied |
| `overkill` | Damage meant beyond the HP the target had at that moment, any source |
| `doomed` | Strikes on an enemy whose queued status pulses alone (HP and statuses at strike time, Flammable as it stands, Corroding stripping armor first, no heals) would already kill it |
| `per_unit` | `(strike + status + retal) / spent` over all seeds; `-` when nothing was spent |
| `status_by` | Status pulse damage (both sources) by status |

### `[levels]` and `[level-ends]`

These appear only when the design file has `[[levels]]`. The rules are in [level-economy.md](level-economy.md).

```
[levels]
level          stage kind     detail         reached win before  spent bitten status  grown corpse  eaten  drunk wasted  after     net
sample-ap2         1 fight    mushroom-1         100 100  10.00   5.00   2.00   0.00   0.00   1.00   1.00   0.00   0.00   4.00   -6.00
sample-ap2         3 fountain Water x3             0   -   0.00   0.00   0.00   0.00   0.00   0.00   0.00   0.00   0.00   0.00   +0.00

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
| `before`, `after` | Body at the stage start and end (after eating or drinking; 0 after a death, what was left after a timeout) |
| `spent`, `bitten`, `status` | Units lost to strikes, bites, the slime's own status pulses |
| `grown` | Blood grown by Regeneration during the fight |
| `corpse` | Units the corpses offered: `eaten` plus what did not fit |
| `eaten`, `drunk` | Units gained from corpses and from the fountain |
| `wasted` | Corpse or fountain units that did not fit into capacity |
| `net` | `corpse + drunk + grown − spent − bitten − status`: the stage's own yield, ignoring capacity (a fountain's undrunk rest is not counted) |

`[level-ends]` has one line per level:

| Column | Meaning |
| --- | --- |
| `end` | Percent of seeds that finished every stage |
| `min` | Average over seeds of the smallest body between stages (start included, losses excluded) |
| `worst` | The smallest body between stages in any seed |
| `wasted` | Average total wasted units per seed |
| `stops` | Where runs ended, `stage<k>:<cause>=seeds`, by count descending; `-` when all finished |

### `[map-levels]` and `[map-level-ends]`

These appear only when the design file has map levels. The rules are in [map-levels.md](map-levels.md).

```
[map-levels]
level          policy     end rooms  rounds fights engaged0 engaged1 engaged2 engaged3+ net_per_fight
map-vision3    strafe       0  1.76   10.22   1.72     0.00    81.40    18.60      0.00         -5.81
map-lure3      lure         0  1.56   11.24   1.80     0.00    88.89    11.11      0.00         -5.56

[map-level-ends]
level          end    min worst  stops
map-vision3      0   0.00     0  room1:melee:Mushroom=8 room1:melee:Mandragora=7 ...
```

| Column | Meaning |
| --- | --- |
| `end` | Percent of seeds that reached the exit |
| `rooms` | Average number of rooms entered |
| `rounds`, `fights` | Average rounds and episodes per run |
| `engaged0` … `engaged3+` | Percent of all episodes whose most aggroed enemies adjacent at once was 0, 1, 2, 3 or more |
| `net_per_fight` | Episode `net` summed over all runs / episodes, signed; `-` without episodes |
| `min`, `worst` | Average and worst smallest body (start and after each episode) |
| `stops` | Where runs ended without the exit: `room<k>:<cause>=seeds`, by count descending |

### CLI for experiments

```bash
cargo run --release -- --design my.toml --no-json --out-txt -     # text to stdout
cargo run --release -- --only-builds lava,water --only-enemies Mushroom,Bat --out-txt my.txt
cargo run --release -- --trace lava mushroom-1 0 1   # turn-by-turn log: build encounter seed ap
cargo run --release -- --trace-map map-vision3 0     # a map level's map and log: level seed
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

The file is a **JSON array of reports**, one per value of `matrix.action_points` × policy in
`matrix.policies` (`data/design.toml`; today actions `[1, 2]` × `newcomer`, `arranger`),
ordered by actions, then policy.
Every report is complete on its own.

```json
[
  {
    "schemaVersion": 1,
    "generatedUtc": "2026-11-25T17:00:00Z",
    "commit": "<git sha the report was built from>",
    "policy": "newcomer",
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
| `policy` | string | How the simulated slime plays: `newcomer` or `arranger` |
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
| `avgUnitsGrown`, `avgSwaps`, `avgNet`, `spentBy`, `bittenBy`, `statusLostBy` | numbers, objects | The `[economy]` slice (objects: name → average) |
| `avgStrikeDmg`, `avgStatusDmg`, `avgRetaliationDmg`, `statusDmgBy`, `avgOverkill`, `avgDoomedStrikes`, `dmgPerUnit` | numbers, object, number or null | The `[damage]` slice |
| `cause` | string or null | The most common cause of a loss; `null` when every fight was won |
| `causes` | object | Every loss cause → number of fights it ended (`{}` when all were won) |

Averages are rounded to 2 decimals.

Loss causes:

| Cause | Meaning |
| --- | --- |
| `melee:<Enemy>` | That enemy's bite took the slime's last unit |
| `status:<Status>` | A status pulse on the slime took its last unit (not possible in v1: enemies apply no statuses) |
| `timeout` | Nobody won within `rules.max_rounds` rounds: a stalemate, for example a Blood slime against a Vampirism bat that heals more than it loses, or a slime down to its last unit next to a Passive enemy that was never hit |
