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
a parent whose branch is the same or has exactly one choice fewer: a branch grows by at most
one city per level and never drops one (checked in `Design::parse`). The row carries everything its branch changes: the
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
  statuses and dominance restart. A body larger than the row's `capacity` keeps its centre
  units and loses the rest from the edge.

## Stops (`[ladder] stops`, run mode only)

After a level listed as a stop's `after` (4, 8, 12, 16), the body each seed carries goes
through the stop before the next level: each reset turns its `from` units into `to` (or drops
them when `to` is absent), edge-most first, at most `max_percent` of the body's units. Then every
policy but the Pus builds (`pus`, `b-pus`, and `b-auto` holding Pus as its affinity) spits out
all its Pus and Dross, free (the smith's house, balance
`resets.md`). Then the empty cells up to the next row's capacity fill with Water at the centre
(the carried units move toward the edge, the edge unit stays the edge). The refill pushes no
unit out, so a Pus build keeps its Pus (balance `sim-3d-spec.md` D4). A `mercury` row whose affinity is
Mercury refills only up to Mercury × 100 / `rules.dominance_percent` (D7).

## Summary

For each mode and threshold, one row per leaf: a ladder row of that difficulty and policy that
is no other row's parent. Its chain is the leaf and its parents down to the lowest level — the
same rows a `run` plays. `furthest_level` is the last level of the unbroken streak of passing
levels from the lowest one. `first_fail_level` is the level that broke it, and `cause` is its
most frequent stop. A leaf below the top level (a branch that ends early) shows its own last
level as `furthest_level` when all its levels pass.

Thresholds must name E, N or H and a map policy (`strafe`, `lure`, `kite`); one with no ladder
rows gives no summary rows.
