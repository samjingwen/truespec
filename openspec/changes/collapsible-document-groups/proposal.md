# Proposal

## Why

The Documents sidebar grows long as current specifications, active changes, archives, and Markdown directories accumulate.
Collapsible groups let reviewers focus on relevant work while keeping the active document visible and preserving their preferred sidebar organization.

## What Changes

- Make every top-level Documents group a native collapsible disclosure.
- Keep current specifications, active changes, and plain Markdown directories open by default.
- Keep archived changes closed by default.
- Preserve each group's open state locally across reloads.
- Preserve nested `delta/` disclosure state independently from its parent change.
- Automatically open all ancestor disclosures when navigation selects a document inside a collapsed group.
- Use the same keyboard-accessible behavior on desktop and narrow drawer layouts.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-reading`: Add persistent, nested, accessible disclosure behavior to document navigation groups.

## Impact

- Affects browser navigation rendering, local-storage preferences, sidebar styling, refresh behavior, and desktop and phone browser tests.
- Does not change document discovery, source paths, current-document identity, review state, or server APIs.
