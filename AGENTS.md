# AGENTS.md

Guidance for AI agents working on the **Slime's Revenge design simulator** (Rust).

## What it is

A model of the game's fight rules that plays a build × encounter matrix and shows the game's
current balance. Its main consumer is the balance session, which reads the plain-text report
(`design-guard.txt`, format agreed with it directly). `design-guard.json` is for the site
(`slimes-revenge/site`). The game itself is
`slimes-revenge/SlimesRevenge` (Unity, C#); its code under `Assets/Scripts/Runtime` is the
source of truth for how the game behaves today. Never edit the game repository from here.

## Layout

```
data/design.toml     # every tunable number (liquids, statuses, enemies, slime, matrix)
src/data.rs          # loads and checks design.toml
src/fight.rs         # one fight: creatures, statuses, strikes, enemy AI, turn order, face-tank policy
src/path.rs          # 8-direction pathing (stands in for the game's A* GridPath)
src/rng.rs           # seeded SplitMix64
src/report.rs        # builds, encounters, the matrix, JSON report
src/level.rs         # stage levels: body carried through fights and fountains
src/map.rs           # map levels: generated rooms, walls, the strafe walk, episodes
src/text.rs          # plain-text report (design-guard.txt)
src/main.rs          # CLI (USAGE): --design, --out, --no-json, --out-txt, --only-*, --trace
reports/             # design-guard.txt committed; CI fails when it is stale
tests/fights.rs      # determinism, hand-checked fights, coverage, contract, stage levels
tests/maps.rs        # map generation, sight, the strafe walk, episodes
docs/                # rules-model.md, report-format.md (index: docs/README.md)
.github/workflows/   # ci.yml: fmt, clippy, test, run; publish to the `latest` release on dev
.claude/rules/       # agent rules
```

## Commands

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --release            # writes design-guard.txt and design-guard.json
cargo run --release -- --no-json --out-txt reports/design-guard.txt   # refresh the committed report
```

Base branch `dev`; pull requests squash-merge into it.

## Gotchas

- **Both report formats are contracts** (`docs/report-format.md`). The text format is agreed
  with the balance session by message: change it only after their ok, and raise its format
  number when columns change. In the JSON, renaming or retyping a field raises `schemaVersion`
  and needs the site updated; adding one is safe.
- **Refresh `reports/design-guard.txt`** in every change that moves the matrix; CI rejects a stale one.
- **Numbers go to `data/design.toml`, rules to code.** A new liquid effect is code in
  `apply_liquid`; its duration and strength are data.
- **Mirror the game, then note the gap.** When the game's docs and its code differ, follow the
  code and add the difference to "Where the docs and the code disagree" in `docs/rules-model.md`.
- **Statuses stack** like the game's `StatusQueue`: never merge two Burnings.
- **Hand-checked tests** in `tests/fights.rs` carry their arithmetic in comments. If a rule
  change breaks one, recompute by hand before changing the expected numbers.
- **Determinism:** randomness goes only through `Rng`; never iterate a `HashMap` where order
  matters for the outcome.
- The `latest` tag and release are moved by CI on every push to `dev`; do not create other
  releases by hand.
