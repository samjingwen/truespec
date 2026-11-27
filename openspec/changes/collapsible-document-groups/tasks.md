# Tasks

## 1. Emit structured navigation from adapters

- [x] 1.1 Extend the document source model with ordered stable navigation segments and leaf labels that contain no structural separators.
- [x] 1.2 Derive recursive OpenSpec hierarchy from semantic roots and artifact paths, including current capabilities, changes, deltas, archives, unfamiliar artifacts, and arbitrary nested directories; add focused Rust tests.
- [x] 1.3 Derive the same recursive hierarchy for plain Markdown directories at arbitrary depth and add focused Rust tests.

## 2. Render the recursive browser tree

- [x] 2.1 Replace browser group and `delta` special cases with a generic recursive tree renderer using native disclosures and adapter-provided IDs, labels, and defaults.
- [x] 2.2 Persist all hierarchy levels independently, tolerate unavailable storage, and preserve document-link and summary focus across catalog refreshes.
- [x] 2.3 Open and persist every supplied ancestor segment for the active document on initial URL load, link navigation, browser history, and source refresh.
- [x] 2.4 Extend desktop and phone browser tests for recursive OpenSpec and Markdown paths, labels without slashes, duplicate folder names, defaults, independent persistence, keyboard operation, and active-document reveal.

## 3. Document and verify

- [x] 3.1 Update README and `AGENTS.md` ownership guidance so adapters own navigation structure and the browser remains source-independent.
- [x] 3.2 Run the Impeccable detector once over changed browser files and run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `openspec validate --all --strict`, `cargo build --locked`, `node tests/browser.mjs`, and `node tests/opencode.mjs`; inspect the public diff for unrelated changes or private paths.

## Workflow follow-up

- Archive the change after implementation and browser review are complete.
- Verify the `document-reading` delta is synced to the accepted specification.
