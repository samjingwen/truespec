# Proposal

## Why

The document toolbar, Review header, persistent general-feedback composer, and full-width send actions consume too much workspace for controls that reviewers use intermittently.
The redundant pairing label adds another footer row without changing delivery state or available actions.

## What Changes

- Reduce the vertical footprint of the document toolbar and Review header while preserving readable paths and accessible controls.
- Remove the redundant `Paired with <integration>` label from the Review footer.
- Replace full-width send actions with compact, right-aligned buttons that retain practical pointer and touch targets.
- Add a separator-mounted icon button that collapses or expands the general-feedback composer.
- Reveal the composer control on pointer hover or keyboard focus and keep it visible for touch and collapsed states.
- Preserve typed general feedback while collapsed, keep submission behavior unchanged, and indicate hidden pending text.
- Remember the composer disclosure preference locally.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-annotations`: Allow the persistent general-feedback composer to collapse without losing pending feedback or changing submission behavior.

## Impact

- Affects embedded browser markup, styling, interaction state, and browser tests.
- Does not change review storage, connector transport, batching, or runtime dependencies.
