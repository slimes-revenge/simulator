# KISS, DRY, YAGNI

Applies to every file. Apply it while writing, then as a separate pass over the full diff (`git diff dev...`) before finishing and before any commit.

## KISS

Ship the smallest change that fixes the current case. No trait, generic, module or helper for a single call site.

```rust
// Bad — a trait with one implementation, used in one place
trait DamageModel { fn apply(&self, c: &mut Creature, n: i32); }

// Good — call the function
damage(d, creature, amount, true, ids);
```

- Prefer direct readable code over many small private functions. A function needs a domain meaning (it mirrors a game method, or names a rule), not just to hide two lines.
- Prefer explicit `match` / `if` for game rules. Keep the rules in `src/fight.rs` next to each other so they read like the game's.
- No new dependency for something the standard library does in a few lines.

## DRY

Extract shared logic only when the same behaviour already exists in two places. A number used in two places goes to `data/design.toml`, not a second constant.

## YAGNI

No placeholders, unused config keys, flags nobody reads, branches for enemies or liquids that do not exist yet. A case the model does not support fails explicitly (an error with the reason from `Design::parse`, or a panic with a message), not silently.

## The simplification pass

1. Read the full diff against `dev` plus the functions it touches.
2. Look for: duplicates of existing code, helpers with one caller, fields or branches nothing reads, code made obsolete by the change.
3. Make the reductions inside the task's flow; record larger cleanups in `docs/plan.md`.
4. Re-run `cargo clippy` and `cargo test`.
