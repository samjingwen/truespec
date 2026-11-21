# Tasks

## 1. Serve and read real documents

- [x] 1.1 Add `truespec serve` with embedded assets, loopback binding, and a printed URL; verify startup and shutdown through a real HTTP process test.
- [x] 1.2 Implement OpenSpec discovery and document grouping; use a self-contained fixture to verify current specs, deltas, unfamiliar artifacts, archives, and nested-directory startup.
- [x] 1.3 Add the Markdown-folder adapter with `--root` and `--docs`; verify it reads a self-contained fixture without OpenSpec and excludes state, symlinks escaping the source, and files outside the selected scope.
- [x] 1.4 Implement shared safe Markdown rendering, document navigation, heading links, and external refresh; verify GFM tables and code, cross-document fragments, and rejection of active content and cross-origin mutations.
- [x] 1.5 Document installation and the two source selection modes; run the documented viewer commands against their fixtures without an OpenSpec executable.
- [x] 1.6 Group active change artifacts under lowercase `change /` headings and nested `delta/` folders; verify underlying OpenSpec paths remain unchanged.
- [x] 1.7 Add an explicit testing-only state reset; verify it starts a fresh review, preserves documents and configuration, and refuses a symlinked state path.
- [x] 1.8 Add panel-header collapse controls, reader-edge reopen tabs, and responsive overlay drawers; verify persistence, focus handling, light dismissal, reader expansion, and mobile safe-area behavior.

## 2. Capture and persist annotations

- [x] 2.1 Add the SQLite schema, workspace-scoped document IDs, and explicit file relinking; verify IDs across adapter switches and process restarts, and verify old paths remain in historical snapshots.
- [x] 2.2 Implement line, paragraph, whole-list, and section anchors and browser selection mapping; test rendered quotes, byte ranges, relocation, and compatibility with saved drafts.
- [x] 2.3 Add line and margin hover targets, a shared inline composer, and one chronological cross-document review feed; verify in-place status changes, source jumps, editing, removal, keyboard access, and restart persistence.
- [x] 2.4 Implement conservative relocation and stale-anchor UI; verify safe shifts, duplicate and missing targets, and preservation of an open composer's text after an external edit.
- [x] 2.5 Add the Annotate switch and viewport-aware selection action; verify above/below placement, annotation-off link navigation, disabled target creation, and existing annotation jumps.
- [x] 2.6 Add the persistent general feedback box above send controls; verify Enter queues an unanchored draft, Shift+Enter inserts a newline, and failures retain text.
- [x] 2.7 Constrain the chronological feed as the Review panel's scroll container; verify expanded original context does not push feedback or send controls offscreen.

## 3. Complete the agent handoff

- [x] 3.1 Persist sessions, immutable batch snapshots, and submission idempotency; verify submission without an agent, stale-draft confirmation, and unchanged snapshots after source edits.
- [x] 3.2 Add **Send** and **Send and End**; test transactional failure recovery, ending an empty review, read-only ended tabs, and explicit new reviews.
- [x] 3.3 Implement the inbox adapter and `poll`, `ack`, and `complete` commands; verify concurrent claims, receipt and renewal, partial completion, lease expiry, and recovery while the viewer is offline.
- [x] 3.4 Show a labeled working spinner and green agent-done icon separately from user-controlled resolution; verify agent completion never resolves an annotation.
- [x] 3.5 Document and exercise the explicit wait, acknowledge, process, complete, and return-to-chat loop, including the idempotency limits for edits made outside TrueSpec.
- [x] 3.6 Show a collapsed, copyable agent prompt for unpaired reviews, preserve its disclosure state across refreshes, and hide it for paired connectors.

## 4. Verify the complete review flow

- [x] 4.1 In a real browser, submit a single batch spanning documents and end the review; take it through a real coding agent where available, or a scripted CLI client otherwise, and verify recovery and correct item correlation after restart.
- [x] 4.2 Exercise the same end-to-end flow with both source adapters, using only self-contained fixtures, and run format, lint, and test checks from a clean checkout.

## 5. Connect coding conversations

- [x] 5.1 Add and document the generic connector protocol; test migration, destination isolation, ordered retries, and receipt idempotency with a fake connector.
- [x] 5.2 Add the optional no-argument `/truespec-review` command; verify two rounds in one isolated OpenCode chat that updates Markdown at the original TrueSpec URL.
- [x] 5.3 Add a common connector setup registry and interactive `truespec setup`; verify list, all, repeatable non-interactive selection, global OpenCode placement, binary-path generation, idempotency, and overwrite protection.
- [x] 5.4 Add durable one-reply-per-batch mirroring; verify OpenCode idle-turn capture, connector idempotency, comment links, unread indication, safe Markdown rendering, persistence, and independent resolution.
- [x] 5.5 Clarify the two-way review policy in connector prompts and public guidance; distinguish questions, explicit changes, clear documentation defects, and ambiguous intent.

## 6. Publish the public repository

- [x] 6.1 Write the public product, workflow, command, storage, security, architecture, limitation, and development guide; add MIT licensing and repository metadata.
- [x] 6.2 Verify a clean checkout, create `samjingwen/truespec` as a public GitHub repository, and push the reviewed `master` branch.
