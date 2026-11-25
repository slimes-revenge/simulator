# Documentation

Context ends; project knowledge must not. Anything whose loss would force someone to re-investigate goes into `docs/`, committed with the code it describes.

| What | Where |
|---|---|
| What the project is, layout, commands, gotchas | `AGENTS.md` |
| How to run it, for a person | `README.md` |
| How the model plays a fight, how it maps to the game code | `docs/rules-model.md` |
| The JSON contract with the site | `docs/report-format.md` |
| Decisions a reader could take for a mistake | `docs/adr/NNNN-<title>.md` |
| Where the work stands, follow-ups, questions | `docs/now.md`, `docs/plan.md`, `docs/questions.md` — git-ignored, local |
| Rules for agents | `.claude/rules/` |

Every note is listed in `docs/README.md`. Write in English, short sentences.

- **Current state, no history.** Describe how it works now; `git log` holds the history.
- **Numbers by name.** Name the `data/design.toml` key that holds a value; repeat the number only when a rule depends on it.
- **Game mapping.** A rule in `src/fight.rs` names the C# method it mirrors. A difference between the game's docs and its code goes into "Where the docs and the code disagree" in `docs/rules-model.md`.
- **Contract changes.** Any change to the output updates `docs/report-format.md` in the same change.

Before finishing a task, check whether it changed something a note, `AGENTS.md`, `README.md` or a rule states, and update them in the same change.
