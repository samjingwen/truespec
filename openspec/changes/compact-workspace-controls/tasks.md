# Tasks

## 1. Define compact workspace behavior

- [x] 1.1 Specify compact toolbar, Review header, footer actions, connection-label removal, and accessible composer disclosure behavior; verify the change passes strict OpenSpec validation.

## 2. Refine embedded controls

- [x] 2.1 Remove the static connection label while preserving connector-aware status and ended-review copy; update browser assertions.
- [x] 2.2 Replace full-width handoff actions with compact right-aligned buttons and verify practical pointer and touch geometry.
- [x] 2.3 Add the separator-mounted general-feedback disclosure control with hover, focus, touch, persistence, pending-text indication, and preserved submission behavior.
- [x] 2.4 Reduce document-toolbar and Review-header height without clipping long paths or reducing existing expand, collapse, and Annotate semantics.

## 3. Verify and deliver

- [x] 3.1 Extend browser coverage for connection-label absence, compact geometry, composer accessibility, persistence, hidden pending text, and desktop, expanded, and phone layouts.
- [x] 3.2 Run the Impeccable detector, complete Rust, OpenSpec, browser, and OpenCode gates, inspect rendered screenshots and the public diff, and complete Hardlook review.

## Workflow follow-up

- Sync the `document-annotations` delta and archive the change after implementation review.
