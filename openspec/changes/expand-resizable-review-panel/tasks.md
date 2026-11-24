# Tasks

## 1. Add Review focus mode

- [x] 1.1 Add the expand and restore icon before **Review**, model transient expanded state independently from panel visibility, and update inert, focus, Escape, edge-toggle, backdrop, desktop-grid, and mobile-drawer behavior.
- [x] 1.2 Extend real-browser tests at desktop and phone widths to verify the dynamic accessible label, full-workspace geometry, hidden and inert neighboring surfaces, Escape restore, exact prior layout restoration, and non-persistence after reload.

## 2. Make Review width adjustable

- [x] 2.1 Add a desktop-only semantic separator on Review's left edge with pointer capture, text-selection suppression, safe width bounds, local-storage preference, viewport re-clamping, and disabled states for collapsed, expanded, and narrow layouts.
- [x] 2.2 Add keyboard resizing with Left Arrow, Right Arrow, and Home plus current ARIA values; extend browser tests for pointer resizing, keyboard resizing, persistence, reset, clamping, and preservation of usable reader width.

## 3. Document and verify the interaction

- [x] 3.1 Update the README responsive-UI description for Review focus mode and persistent desktop width without adding implementation-specific instructions.
- [x] 3.2 Run the Impeccable detector once over changed browser targets, inspect the final UI at desktop and phone widths, and run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `openspec validate --all --strict`, `cargo build --locked`, `node tests/browser.mjs`, and `node tests/opencode.mjs`.

## Workflow follow-up

- Archive the change after implementation and visual review are complete.
- Verify the `document-reading` delta is synced to the accepted specification.
