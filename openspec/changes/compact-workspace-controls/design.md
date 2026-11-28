# Design

## Context

TrueSpec uses one embedded browser workspace across desktop columns, expanded Review, and narrow overlay drawers.
The current document toolbar and Review header use generous vertical padding, the send actions divide the complete footer width, and the general-feedback textarea is always expanded.
Connector identity remains available in queued status and ended-review copy even when the static pairing label is removed.

## Goals / Non-Goals

**Goals:**

- Return vertical space to reviewed content and the chronological feedback feed.
- Preserve clear hierarchy, focus visibility, touch usability, and long-path wrapping.
- Make the general-feedback composer collapsible without hiding or losing pending work.
- Keep the compact treatment consistent across normal Review, expanded Review, and phone drawers.

**Non-Goals:**

- Change feedback semantics, button labels, batching, or connector behavior.
- Add a resizable composer or another persistent application panel.
- Change the height of the primary TrueSpec application header.

## Decisions

### Compact the document toolbar without fixing its height

Reduce the reader's top inset, toolbar block padding, path line height, and Annotate padding.
Keep the toolbar content-driven so long paths can wrap instead of clipping or overlapping the Annotate switch.
Target a single-line visual height near 40 to 44 pixels while allowing narrow layouts to grow when content requires it.

### Compact the Review header globally

Reduce Review header padding and heading size in normal, expanded, and narrow modes.
Retain the existing 2rem expand and collapse controls and focus treatment.
Target an ordinary height near 44 to 48 pixels rather than adding an expanded-mode exception.

### Use content-width footer actions

Render the handoff row as a right-aligned flex group.
Use compact typography and padding with a minimum 2.25rem action height, preserving one-row layout at the minimum Review width.
Keep disabled, hover, focus, and primary-action styling unchanged.

### Remove only the redundant connection label

Remove its DOM node, renderer call, and unused style.
Keep connector identity in queued status messages, ended-review guidance, and server state.

### Mount a disclosure control on the composer separator

Place an authored chevron icon button over the composer's top border.
The button remains in the tab order and exposes `aria-expanded`, `aria-controls`, an action label, and a title.
It is faint at rest, fully visible on hover or focus, and always visible on touch devices and while collapsed.
The chevron rotates over 140ms to communicate collapse direction without animating layout height.

### Preserve hidden pending feedback

Store the collapsed preference in local storage and default to expanded when no preference exists.
Collapsing hides the textarea and hint but never clears or disables the textarea.
Pending text continues enabling **Send**, is still included by **Send** and **Send and End**, and adds an accent indicator plus a draft-aware accessible label to the collapsed control.
Expanding restores the same text and focuses the textarea.

## Risks / Trade-offs

- [A hover-only control is inaccessible on touch and easy to miss with a keyboard] -> Keep it focusable, reveal it on focus, and show it persistently on touch and collapsed states.
- [Collapsed text can be forgotten] -> Preserve submission behavior and show both visual and accessible pending indicators.
- [Aggressive toolbar compaction can break long paths] -> Avoid fixed heights and verify real long paths at desktop and phone widths.
- [Compact actions can become difficult touch targets] -> Keep a minimum 2.25rem height and verify geometry at the minimum Review width.
