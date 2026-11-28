# Tasks

## 1. Enforce fresh-state compatibility

- [x] 1.1 Refuse non-current review database versions without mutation and verify the error names the explicit reset command.
- [x] 1.2 Bootstrap new databases directly at the current schema while retaining append-only schema files for fresh creation.

## 2. Add explicit state reset

- [x] 2.1 Add `init --reset-state`, authenticated route forgetting, safe repository-local deletion, and fresh workspace registration.
- [x] 2.2 Verify symlink refusal, permanent-loss warning, old-state preservation without the flag, and a new workspace identity with the flag.

## 3. Document and verify

- [x] 3.1 Update accepted initialization behavior and public upgrade guidance without overloading `--force`.
- [x] 3.2 Run formatting, Clippy, Rust tests, strict OpenSpec validation, browser tests, OpenCode tests, public-repository audit, and exact-head review.
