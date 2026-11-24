# Tasks

## 1. Make pending general feedback transactional

- [x] 1.1 Add optional `general_body` to submission identity and create its general comment, feed entry, snapshot, and batch item inside the existing SQLite submission transaction after queued drafts.
- [x] 1.2 Add focused Rust coverage for general-only submission, mixed draft ordering, send-and-end atomicity, replay idempotency, validation failure, and preservation of existing request compatibility.

## 2. Include pending text from the browser

- [x] 2.1 Treat non-empty trimmed general text as sendable state, include it in the stable submission payload, and clear it only after success while preserving Enter-to-queue and empty-end behavior.
- [x] 2.2 Add real-browser regressions for direct **Send**, direct **Send and End** with `"apply"`, mixed drafts, failure preservation, enabled controls, one final connector batch, and no duplicate item after retry.

## 3. Verify the fix

- [x] 3.1 Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `openspec validate --all --strict`, `cargo build --locked`, `node tests/browser.mjs`, and `node tests/opencode.mjs`; inspect the final diff for unrelated UI or protocol changes.

## Workflow follow-up

- Archive the change after implementation and regression review are complete.
- Verify both capability deltas are synced to accepted specifications.
