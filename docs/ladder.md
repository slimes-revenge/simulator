# The ladder

The game's 20 levels as one climb: each level at each difficulty (E, N, H), for each policy and
each branch of city choices. It answers how far each difficulty × policy gets, and where it
first fails. The spec was agreed with the balance session, which generates the rows. Code:
`src/ladder.rs`. Output: `[ladder]` and `[ladder-summary]` (`docs/report-format.md`).

## Input

A ladder row is a map level ([map-levels.md](map-levels.md)) with
`ladder = { level, difficulty, branch }`:

| Key | Meaning |
| --- | --- |
| `level` | The ladder level, from 1 |
| `difficulty` | `E`, `N` or `H` |
| `branch` | The city choices made so far, joined with `-` (e.g. `Lava-Mercury`); empty (default) before the first city |

Rows of one difficulty and policy form a tree. A row **continues** a branch when its branch is
that branch, or extends it by more choices (`Lava-Mercury` continues `Lava` and the empty
branch). A row's **parent** is the row one level lower, same difficulty and policy, with the
longest branch it continues. Every row above the lowest level of its difficulty × policy needs
a parent (checked in `Design::parse`). The row carries everything its branch changes: the
start body, river liquids, the enemies (carriers of a chosen liquid are their own
`[[enemies]]` rows).

`[ladder] thresholds` hold the pass rules: `{ difficulty, policy, min }` or `{ …, max }`, in
percent of `end`.

## Modes

Seed `s` plays map seed `s` on every level in both modes, so the modes agree on the lowest
level.

- **`level`** — each row from its own `start`, like any map level.
- **`run`** — the lowest level starts from its `start`. Each higher row starts each seed with
  the body that seed had at its parent row's exit, after drinking the river. A seed that died
  stops there; `end` counts it as failed on every later level. Only the body carries over:
  statuses and dominance restart.

## Summary

For each mode, threshold and full branch (a branch no other row continues): the chain of rows
the branch continues, by level. `furthest_level` is the last level of the unbroken streak of
passing levels from the lowest one. `first_fail_level` is the level that broke it, and `cause`
is its most frequent stop.
