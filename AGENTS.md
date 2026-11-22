# TrueSpec

## Purpose

TrueSpec is a local-first Rust CLI for reviewing existing Markdown with coding agents.
It serves an embedded browser UI, stores review state separately, and delivers durable feedback batches through replaceable harness connectors.

## Source of truth

- Accepted product behavior belongs in `openspec/specs/`.
- Proposed behavior belongs in `openspec/changes/<change>/specs/` until archive.
- Archived OpenSpec changes preserve proposal, design, task, and decision history.
- The accepted coding-harness contract belongs in `openspec/specs/connector-protocol/spec.md`.
- `README.md` is the public user and command guide.
- Source Markdown remains authoritative for reviewed documents.
- Review comments, batches, replies, delivery state, and resolution state never belong in source Markdown.

## Ownership

- `src/source.rs` owns the source-adapter registry and document-source implementations.
- `src/project.rs` owns versioned project configuration and repository-root validation.
- `src/initialize.rs` owns integration selection, complete preflight, and generated-file installation.
- `src/integration.rs` owns the coding-harness integration registry and installation artifacts.
- `src/markdown.rs` owns safe Markdown rendering and rendered-to-source mapping.
- `src/anchor.rs` owns annotation evidence and conservative relocation.
- `src/review.rs` owns SQLite review state and immutable batch snapshots.
- `src/inbox.rs` owns connector leasing, receipts, retries, and delivery envelopes.
- `src/web.rs` owns the loopback HTTP API and embedded browser assets.
- `web/` owns the embedded browser interface.
- `.opencode/plugins/truespec/` owns the optional OpenCode connector.
- `skills/truespec-review/` owns the portable connector-delivered feedback instructions.

## Invariants

- Keep document-source adapters independent from coding-harness connectors.
- Keep source and integration adapters registered at compile time and independent from the shared review core.
- Treat `truespec/config/project.json` as shared project configuration and integration selection as user-local state.
- Keep the shared review core independent from OpenSpec and any one agent harness.
- Read OpenSpec artifacts in place without merging deltas or changing OpenSpec workflow state.
- Preserve submitted feedback and source context as immutable snapshots.
- Relocate an annotation only when its source match is reliable and unique.
- Mark ambiguous targets `Outdated` instead of attaching them to another occurrence.
- Keep moved-file history at its original missing path; never infer identity from matching content.
- Keep agent delivery or completion separate from reviewer resolution.
- Never resolve a comment automatically after an agent reply.
- Bind the viewer to loopback by default.
- Serve only discovered Markdown documents.
- Do not execute raw HTML, scripts, or externally loaded images from reviewed Markdown.
- Keep `truespec/.state/` and `truespec/config/` outside document-serving routes.
- Never require OpenSpec, OpenCode, Lavish, or Node.js at TrueSpec runtime.
- Keep `init` as the only public product command and keep connector transport and viewer operations hidden and versioned.
- Do not add browser-based source editing, hosted synchronization, multi-user collaboration, or arbitrary agent launching without an accepted OpenSpec change.

## Browser and connector behavior

- Browser assets are embedded at Rust compile time.
- Rebuild the Rust binary after changing files under `web/`.
- Start browser reviews only through an initialized, paired integration.
- Do not add a standalone browser prompt or public manual inbox workflow.
- Keep the chronological Review feed stable when comment delivery state changes.
- Render mirrored agent replies as safe, read-only Markdown.
- Preserve one idempotent agent reply per feedback batch.
- Keep reusable feedback interpretation in `skills/truespec-review/SKILL.md`, not repeated in connector messages.
- Keep connector commands versioned and usable while the browser server is offline.
- Treat OpenCode as one connector implementation, not as a core dependency.
- Preserve keyboard, narrow-viewport, safe-area, and reduced-motion behavior for UI changes.

## State and migrations

- Use SQLite transactions for draft submission, batch creation, ending reviews, and reply recording.
- Keep migrations append-only and advance `PRAGMA user_version`.
- Do not rewrite a migration that may have been applied.
- Do not hand-edit `Cargo.lock`.
- Use hidden connector state reset only for explicit local testing.

## Verification

Run the ordinary gate:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
openspec validate --all --strict
```

For browser or review-flow changes, also run:

```sh
cargo build --locked
node tests/browser.mjs
```

For connector, setup, session, or OpenCode changes, also run:

```sh
node tests/opencode.mjs
```

Browser changes require inspection at desktop and phone widths.
Do not claim platform or harness behavior that the executed checks did not verify.

## Delivery

- The default branch is `master`.
- Keep changes scoped to the active OpenSpec change.
- Update the relevant OpenSpec artifacts and public README when behavior changes.
- Review the intended Git diff before committing.
- Never add an agent identity as a commit co-author.
