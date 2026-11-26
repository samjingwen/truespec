# Tasks

## 1. Add persistent sidebar disclosures

- [x] 1.1 Convert top-level document groups to native disclosures with source-aware defaults, custom chevrons, and unchanged document ordering and labels.
- [x] 1.2 Persist top-level and nested `delta/` states independently, tolerate unavailable storage, and preserve document-link and summary focus across catalog refreshes.

## 2. Keep active navigation visible

- [x] 2.1 Open and persist every ancestor disclosure for the active document on initial URL load, link navigation, browser history, and source refresh without changing the displayed document.
- [x] 2.2 Extend desktop and phone browser tests for defaults, independent toggles, nested deltas, reload persistence, active-document reveal, keyboard operation, and narrow-drawer behavior.

## 3. Document and verify

- [x] 3.1 Update the README responsive-workspace feature summary to mention collapsible, remembered document groups.
- [x] 3.2 Run the Impeccable detector once over changed browser files and run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `openspec validate --all --strict`, `cargo build --locked`, `node tests/browser.mjs`, and `node tests/opencode.mjs`; inspect the public diff for unrelated changes or private paths.

## Workflow follow-up

- Archive the change after implementation and browser review are complete.
- Verify the `document-reading` delta is synced to the accepted specification.
