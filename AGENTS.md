# AGENTS.md

Guidance for AI agents working on the **Slime's Revenge design simulator** (Rust).

## What it is

A model of the game's fight rules that plays a build × encounter matrix and writes
`design-guard.json` for the site (`slimes-revenge/site`). The game itself is
`slimes-revenge/SlimesRevenge` (Unity, C#); its code under `Assets/Scripts/Runtime` is the
source of truth for how the game behaves today. Never edit the game repository from here.

## Layout

```
data/design.toml     # every tunable number (liquids, statuses, enemies, slime, matrix)
src/data.rs          # loads and checks design.toml
src/fight.rs         # one fight: creatures, statuses, strikes, enemy AI, turn order, face-tank policy
src/path.rs          # 8-direction pathing (stands in for the game's A* GridPath)
src/rng.rs           # seeded SplitMix64
src/report.rs        # builds, encounters, the matrix, JSON report, console table
src/main.rs          # CLI: --design, --out
tests/fights.rs      # determinism, hand-checked fights, coverage, contract
docs/                # rules-model.md, report-format.md (index: docs/README.md)
.github/workflows/   # ci.yml: fmt, clippy, test, run; publish to the `latest` release on dev
.claude/rules/       # agent rules
```

## Commands

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --release            # writes design-guard.json
```

Base branch `dev`; pull requests squash-merge into it.

## Gotchas

- **The report format is a contract.** The site builds against `docs/report-format.md`.
  Renaming or retyping a field raises `schemaVersion` and needs the site updated; adding one is safe.
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
