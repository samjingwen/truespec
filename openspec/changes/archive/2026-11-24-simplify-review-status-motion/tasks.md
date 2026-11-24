# Tasks

## 1. Simplify visible review status

- [x] 1.1 Remove submitted-item **Resolve / Reopen** actions and **Resolved / Unresolved** badges while preserving draft actions, agent progress ordering, and source-freshness warnings.
- [x] 1.2 Update desktop and phone browser coverage for queued, done, mirrored-reply, outdated, missing-source, and historical resolved values; verify no resolution label or action appears.

## 2. Apply motion consistently

- [x] 2.1 Remove all reduced-motion CSS and browser branching so spinners and interface transitions use normal motion regardless of `prefers-reduced-motion`.
- [x] 2.2 Add browser assertions under a reduced-motion environment that the spinner retains its animation and drawer and switch transitions remain enabled.

## 3. Align contracts and guidance

- [x] 3.1 Update README status guidance and `AGENTS.md` invariants to remove reviewer resolution and reduced-motion preservation while retaining database compatibility notes where useful.
- [x] 3.2 Run the Impeccable detector once over changed browser files and run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `openspec validate --all --strict`, `cargo build --locked`, `node tests/browser.mjs`, and `node tests/opencode.mjs`; inspect the public diff for stale resolution and reduced-motion references outside historical archives.

## Workflow follow-up

- Archive the change after implementation and review are complete.
- Verify all four capability deltas are synced to accepted specifications.
