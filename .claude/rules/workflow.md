# Task workflow

1. **Start** — understand the task. Write the acceptance criteria before writing code; if the task has none, propose them. If a requirement is ambiguous, ask; do not guess.
2. **Branch** — before the first edit, switch to a branch for the task created from **`dev`**. Without a given name use `wip-<short-topic>`.
3. **Implement** — the smallest change that satisfies the criteria. Follow the rules in `.claude/rules/` and the matching note in `docs/`.
4. **Verify** — `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo run --release` when the rules or the data change (read the matrix: did it move where you expected?). Tests check the acceptance criteria and the game's rules, not the code you just wrote.
5. **Simplify** — KISS / DRY / YAGNI pass over the whole diff (`kiss-dry-yagni` rule). Re-run the affected checks.
6. **Document** — `docs/` notes, `AGENTS.md`, `README.md` (`documentation` rule). A change to the JSON output updates `docs/report-format.md` in the same PR.
7. **Report** — what changed, the checks run with their outcome, how the matrix moved, what is left.

Problems noticed outside the task's scope are not fixed silently and not dropped: report them and add them to `docs/plan.md`.

## Definition of Done

- Every acceptance criterion is met and has a test or a recorded manual check.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` were **run** and **passed** in this session.
- No new `#[ignore]`, `#[allow(...)]` or weakened assertions to get green.
- The report contract is unchanged, or `schemaVersion` is raised and the site is told.
- KISS / DRY / YAGNI pass done; documentation updated.

## Reporting

Keep three statements apart: **test written**, **test run**, **test passed**. When a check was not run, say so and why. When a check failed, show the failing output.

## Git

- Never write code on `dev` or someone else's branch. Check `git branch --show-current` before every commit and push.
- Conventional commits (`fix:`, `feat:`, `feat!:`, `chore:`, `docs:`, `test:`, `ci:`), one logical change per commit.
- A pull request targets `dev` and says what changes and why, how it was verified, and what is not verified. Squash merge.
- Do not force-push shared branches or rewrite pushed history. The `latest` tag is moved only by CI.
- After a merge, delete the task branch locally (`gh pr list --state merged --head <branch>`, then `git branch -D <branch>`).
