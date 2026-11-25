# Slime's Revenge design simulator

The design guard for [Slime's Revenge](https://github.com/slimes-revenge/SlimesRevenge): it plays
fights by the game's rules many times and reports which slime builds win against which
encounters, so a change to a basic number cannot silently break the design.

- Every tunable number is in [`data/design.toml`](data/design.toml).
- The rules and how they map to the game code: [`docs/rules-model.md`](docs/rules-model.md).
- The output contract the site reads: [`docs/report-format.md`](docs/report-format.md).

## Run

Needs a stable Rust toolchain (`rustup`).

```bash
cargo run --release
```

Prints a win-rate matrix per action-point value and writes `design-guard.json` (one JSON array,
one report per `matrix.action_points` value). Options: `--design <path>` for another design
file, `--out <path>` for another output file.

```bash
cargo test
```

## What v1 plays

- **Builds:** the starting mix (2 Water, Oil, Poison, Acid, Lava) and 10 units of each liquid.
- **Encounters:** 1, 2 and 3 of each live enemy (Bat, Mushroom, Undine, Cactus, Mandragora,
  Eye), in a column 5 cells east of the slime at (1, 5).
- **Policy:** face-tank — walk to the nearest enemy, strike with the liquid held most, wait
  when the last unit cannot be spent.
- **Action points:** 1 per round (the game today) and 2 (the design).
- **Seeds:** 50 per cell; the same seed gives the same fight.

## Published report

CI publishes the report of every push to `dev` as an asset of the rolling `latest` release:
<https://github.com/slimes-revenge/simulator/releases/download/latest/design-guard.json>
