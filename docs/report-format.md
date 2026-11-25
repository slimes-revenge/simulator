# Report format (`design-guard.json`)

The contract between the simulator and the site (`slimes-revenge/site`). The site draws this
file and computes nothing itself. Changing a field's name, type or meaning raises
`schemaVersion`; adding a field does not.

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
| `cause` | string or null | The most common cause of a loss; `null` when every fight was won |
| `causes` | object | Every loss cause → number of fights it ended (`{}` when all were won) |

Averages are rounded to 2 decimals.

Loss causes:

| Cause | Meaning |
| --- | --- |
| `melee:<Enemy>` | That enemy's bite took the slime's last unit |
| `status:<Status>` | A status pulse on the slime took its last unit (not possible in v1: enemies apply no statuses) |
| `timeout` | Nobody won within `rules.max_rounds` rounds: a stalemate, for example a Blood slime against a Vampirism bat that heals more than it loses, or a slime down to its last unit next to a Passive enemy that was never hit |
