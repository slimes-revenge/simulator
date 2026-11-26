# Slime's Revenge design simulator

The design guard for [Slime's Revenge](https://github.com/slimes-revenge/SlimesRevenge): it plays
fights by the game's rules many times and reports which slime builds win against which
encounters, so a change to a basic number cannot silently break the design.

- Every tunable number is in [`data/design.toml`](data/design.toml).
- The rules and how they map to the game code: [`docs/rules-model.md`](docs/rules-model.md).
- The two report formats (plain text for the balance session, JSON for the site):
  [`docs/report-format.md`](docs/report-format.md).

## Run

Needs a stable Rust toolchain (`rustup`).

```bash
cargo run --release
```

Writes `design-guard.txt` (plain text, diffable) and `design-guard.json` (for the site).
`reports/design-guard.txt` is the committed copy; after changing rules or numbers, regenerate it:

```bash
cargo run --release -- --no-json --out-txt reports/design-guard.txt
```

Other options (another design file, filters, a turn-by-turn trace of one fight):
see [`docs/report-format.md`](docs/report-format.md#cli-for-experiments).

```bash
cargo test
```

## What v1 plays

- **Builds:** the starting mix (3 Water, Poison, Acid, Lava) and 10 units of each liquid.
- **Encounters:** 1, 2 and 3 of each live enemy (Bat, Mushroom, Undine, Cactus, Mandragora,
  Eye), in a column 5 cells east of the slime at (1, 5).
- **Policies:** newcomer — walk to the nearest enemy, strike with the unit on the body's edge,
  wait when the last unit cannot be spent, never swap; arranger — the same with at most one
  swap of the two edge units before a strike; kite — strike, then step out of reach on the
  round's last action.
- **Action points:** 1 per round (the game today) and 2 (the design).
- **Seeds:** 50 per cell; the same seed gives the same fight.

- **Levels:** `[[levels]]` in the design file run stage by stage (fights, then eating the
  corpses, and fountains) and report the body budget: [`docs/level-economy.md`](docs/level-economy.md).
  A level with a `map` is a generated chain of rooms with walls and roaming enemies instead:
  [`docs/map-levels.md`](docs/map-levels.md).

## Published report

CI publishes the reports of every push to `dev` as assets of the rolling `latest` release:
<https://github.com/slimes-revenge/simulator/releases/download/latest/design-guard.txt> and
<https://github.com/slimes-revenge/simulator/releases/download/latest/design-guard.json>
