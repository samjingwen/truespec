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

- `src/source.rs` owns the source-adapter registry, document-source implementations, and structured navigation hierarchy.
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
- Derive navigation segments, stable folder IDs, leaf labels, and default disclosure state inside source adapters; keep the browser renderer source-independent.
- Treat `truespec/config/project.json` as shared project configuration and integration selection as user-local state.
- Keep the shared review core independent from OpenSpec and any one agent harness.
- Read OpenSpec artifacts in place without merging deltas or changing OpenSpec workflow state.
- Preserve submitted feedback and source context as immutable snapshots.
- Relocate an annotation only when its source match is reliable and unique.
- Mark ambiguous targets `Outdated` instead of attaching them to another occurrence.
- Keep moved-file history at its original missing path; never infer identity from matching content.
- Keep agent progress and source freshness as the complete visible feedback status.
- Preserve legacy resolution data for database compatibility, but do not expose reviewer-resolution controls or badges.
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
- Preserve keyboard, narrow-viewport, and safe-area behavior for UI changes.
- Apply normal interface motion regardless of the operating-system reduced-motion preference.

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

### Authority and scope

- The default branch is `master`.
- Planning and read-only investigation may happen in the primary checkout, but every repository edit must happen in a dedicated Worktrunk worktree.
- Own the applicable sequence from requirements and implementation through verification, review, GitHub pull request, merge, safe primary-checkout synchronization, and session return.
- A plan-only, review-only, PR-only, human-review, or other explicit task limit takes precedence over the complete sequence.
- A merge does not authorize a release, package publication, deployment, production operation, or other external effect.
- Keep changes scoped to the active OpenSpec change.
- Update the relevant OpenSpec artifacts and public README when behavior changes.

### Isolate and implement

- Before editing, inspect status, worktrees, the actual default branch, and active ownership.
- Record the primary checkout path and default branch during initial inspection so final synchronization does not depend on rediscovery.
- Read the installed `wt switch --help` and inspect Worktrunk hooks before creating a worktree.
- Fetch the intended base and update the primary checkout only with fast-forward semantics when it is safe.
- Create a dedicated branch and worktree from the intended base with Worktrunk, then verify its path, branch, and merge base before the first edit.
- Continue related work in its existing worktree instead of creating a competing worktree or moving another task's uncommitted changes.
- Preserve unrelated user work, local state, review state, credentials, logs, and machine-local records.
- Never read private environment files or authentication caches.
- Never commit secrets, local paths, databases, logs, screenshots, build output, or private identifiers.

### Verify and review

- Run the ordinary gate and every applicable browser or OpenCode integration gate before delivery.
- Reproduce reported bugs through the user-facing path before fixing them.
- Preserve failing evidence, fix causes rather than weakening checks, and rerun checks invalidated by each fix.
- Review the complete base-to-head diff.
- Use the installed Hardlook workflow for every implementation change, record its coverage and limitations, address accepted findings, and rerun invalidated checks.
- A partial review, failed check, unavailable tool, or passing rerun without a substantiated fix is not clean acceptance.

### Deliver through GitHub

- Review status, all intended commits, and the complete diff before staging only the task's files.
- Never add an agent identity as a commit co-author.
- Push the feature branch and open a GitHub pull request.
- Include decisions, exact-head verification, Hardlook coverage, limitations, and relevant migration or release notes in the pull request.
- Require the hosted `Quality` and `Browser and OpenCode` checks and resolve every pull-request conversation.
- Merge through GitHub only after actual required checks and reviews pass.
- Verify the merged state and exact merge SHA.

### Synchronize and return

- Before synchronizing, inspect the primary checkout for dirty, untracked, ignored, or actively owned files that could collide.
- Fetch the remote and pull the actual default branch with fast-forward-only semantics when safe.
- Never stash, reset, clean, force-update, or rebase local divergence to make synchronization succeed.
- Preserve user-owned files and report any collision or synchronization blocker.
- Verify that local `master` equals the freshly fetched `origin/master` and contains the merge while retaining any later remote commits.
- If primary synchronization is blocked, remain in the implementation worktree and report the blocker.
- After synchronization, use a verified native workspace-move operation when the active coding harness provides one.
- Invoke a harness workspace move alone at a safe execution boundary, then verify the destination path, branch, HEAD, merge ancestry, and status in a later operation.
- For OpenCode, use the current Code Mode tool catalog entry `tools.opencode.session.move`; follow its injected schema instead of guessing arguments.
- Do not substitute shell `cd`, Worktrunk shell integration, or a new harness process for a harness-native session move.
- Add a future harness mapping only after verifying its native transition tool, exact invocation boundary, and post-transition checks.
- If no verified native transition is available, remain in the implementation worktree and report the limitation.
- Do not remove the implementation worktree automatically.
- Report the pull request, merge SHA, checks, review coverage, primary path, branch, synchronized SHA, and any blocker honestly.
