# Proposal

## Why

The public repository contains one rewritten historical migration and several stale references to manual polling after the public workflow moved to `truespec listen`.
Correcting these inconsistencies now preserves upgrade safety and keeps the browser, CLI, accepted behavior, and public guide aligned.

## What Changes

- Restore the published version 3 migration and leave nullable manual-reply support exclusively in the append-only version 4 migration.
- Add regression coverage that upgrades a real version 3 database to version 4 without losing connector replies.
- Replace user-facing manual polling language with listening language while retaining connector-internal polling terminology.
- Clarify that OpenCode may answer a review question without changing Markdown.
- Remove placeholder `.gitkeep` files from OpenSpec directories that now contain accepted specs and archived changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `review-handoff`: Describe the manual handoff as a listen, acknowledge, process, and reply loop and keep question-only feedback distinct from requested Markdown changes.

## Impact

- Affects the version 3 migration fixture, version 4 upgrade tests, manual inbox status text, browser disclosure copy, browser tests, README guidance, and the `review-handoff` specification.
- Does not change the current version 4 database shape, connector protocol, CLI command surface, or runtime dependencies.
