# Tasks

## 1. Restore migration integrity

- [x] 1.1 Restore the published version 3 `agent_replies.connector_id` constraint and verify the diff leaves nullable connector IDs exclusively in `schema-v4.sql`.
- [x] 1.2 Add a populated version 3 to version 4 upgrade regression test that verifies `PRAGMA user_version`, reply and feed preservation, and final connector-ID nullability; run the focused Rust test.

## 2. Align manual review language

- [x] 2.1 Replace user-facing manual polling copy with listening copy in the browser and CLI guidance, preserve connector-internal polling terms, update browser assertions, and run `cargo build --locked && node tests/browser.mjs`.
- [x] 2.2 Update the README manual disclosure and OpenCode walkthrough so questions need not modify Markdown, then verify no stale user-facing polling phrase remains outside archived history or connector internals.

## 3. Remove obsolete placeholders and verify

- [x] 3.1 Remove the redundant OpenSpec `.gitkeep` files and verify accepted specs and archived changes still keep both directories tracked.
- [x] 3.2 Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `openspec validate --all --strict`, and `node tests/opencode.mjs`; review the final diff for scope and generated or private files.

## Workflow follow-up

- Archive the change after implementation and review are complete.
- Verify the archived delta updates the accepted `review-handoff` specification.
