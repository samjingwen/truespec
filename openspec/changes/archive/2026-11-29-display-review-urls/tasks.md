# Tasks

## 1. Present canonical review URLs

- [x] 1.1 Update the OpenCode command to emit only the selected canonical URL followed by the distinct local canonical URL without launching a browser, and verify the isolated OpenCode test asserts exact tailnet and local-mode synthetic message text, ordering, and absence of labels
- [x] 1.2 Document the line-only manual-open `/truespec-review` output in README and verify the generated plugin installation test covers the updated presentation code and absence of browser-launch helpers

## 2. Integration verification

- [x] 2.1 Run formatting, Clippy, Rust tests, strict OpenSpec validation, the isolated OpenCode flow, the public-repository audit, and exact-head Hardlook; address findings and rerun every invalidated check

## Workflow follow-up

- Sync the connector protocol delta and archive the change after implementation review.
- Require hosted `Quality` and `Browser and OpenCode` checks before merge.
