# Tasks

## 1. Configure source adapters and projects

- [x] 1.1 Add a compile-time source-adapter registry around `DocumentSource`, replace central source creation matches, and verify OpenSpec and Markdown detection, opening, and adapter-independent IDs with focused Rust tests.
- [x] 1.2 Add versioned `truespec/config/project.json` parsing and writing with canonical workspace-relative `source.adapter` and `source.path`; verify malformed versions, unknown adapters, missing paths, excluded paths, and escaping symlinks are rejected without writes.
- [x] 1.3 Implement repository-root `truespec init` source detection and `--source` / `--source-path` overrides; verify exact-root enforcement, OpenSpec preference, Markdown fallback, idempotent reinitialization, and preservation of existing state in CLI tests.

## 2. Select and install integrations

- [x] 2.1 Add a compile-time integration registry and two-phase `InitPlan` for project configuration plus generated artifacts; verify destination deduplication, complete preflight, conflict aggregation, force replacement, atomic per-file writes, and interrupted-install repair with fake adapters.
- [x] 2.2 Add a terminal multi-select and repeatable `--integration` / `--all` options; verify duplicate selection, unknown IDs, non-interactive omission, all-selection, and no-write failure behavior.
- [x] 2.3 Convert OpenCode setup into an integration adapter that installs the generated plugin and global `truespec-review` skill; extend CLI tests for first install, matching rerun, partial repair, differing-file protection, and `--force`.

## 3. Simplify commands and paired startup

- [x] 3.1 Reduce normal CLI help to `init`, help, and version; move viewer startup and transport under the hidden versioned connector command, remove public manual inbox and relink commands, and verify public help and rejected legacy invocations.
- [x] 3.2 Make connector attachment load and validate initialized project configuration rather than accepting a source flag; verify both configured source adapters start through hidden viewer operations while unsupported protocol versions remain side-effect free.
- [x] 3.3 Update the OpenCode plugin to call the hidden initialized operations and verify `/truespec-review` opens both OpenSpec and Markdown projects in isolated integration tests.

## 4. Deliver feedback through the skill

- [x] 4.1 Add the portable `skills/truespec-review/SKILL.md` with narrow connector-delivery triggers, source-context safety, current-file inspection, question/change intent handling, ambiguity guidance, and final-response expectations; verify frontmatter, trigger wording, and the absence of manual inbox commands.
- [x] 4.2 Replace repeated OpenCode feedback instructions with one `@truespec-review` invocation plus immutable batch JSON; verify delivery idempotency, stable receipts, automatic reply mirroring, and independent reviewer resolution in the OpenCode integration test.

## 5. Preserve legacy queued feedback

- [x] 5.1 Transactionally adopt incomplete connector-free batches when the first initialized integration attaches, preserving snapshots, completed items, replies, feed order, and attempts while clearing obsolete leases; verify unreceived, leased, partially completed, and fully handled batches.

## 6. Remove the standalone browser workflow

- [x] 6.1 Remove **Agent not listening?**, manual prompt generation, copy controls, persisted disclosure state, and related styles while keeping paired queue and ended messaging accessible.
- [x] 6.2 Update desktop and phone browser coverage to require paired reviews, confirm no standalone handoff is rendered, and exercise both source adapters through `cargo build --locked && node tests/browser.mjs`.

## 7. Publish the initialized workflow

- [x] 7.1 Rewrite README installation, quick starts, workflow, command reference, storage, and migration guidance around `truespec init`, multi-integration selection, `--source-path`, and harness-started reviews; verify every documented command against an isolated fixture.
- [x] 7.2 Update `AGENTS.md` ownership and invariants for project configuration, source and integration registries, bundled skills, hidden connector operations, and moved-file behavior; verify no guidance references removed public commands.

## 8. Verify the complete change

- [x] 8.1 Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `openspec validate --all --strict`, `node tests/browser.mjs`, and `node tests/opencode.mjs`; inspect the final public diff for generated files, private paths, stale command references, and unintended runtime dependencies.

## Workflow follow-up

- Archive the change after implementation and review are complete.
- Verify all five capability deltas are synced to accepted specifications.
