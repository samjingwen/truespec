# Design

## Context

The browser workspace is a three-column CSS grid on desktop and switches Documents and Review to overlay drawers below 60rem.
Panel open state is stored in local storage, while Review currently uses a fixed responsive column width.
The existing controls already manage inert state, focus restoration, unread replies, safe-area padding, and Escape dismissal.

## Goals / Non-Goals

**Goals:**

- Give Review a clear temporary focus mode without losing the previous layout.
- Let desktop reviewers choose and retain a practical sidebar width.
- Make maximize, restore, and resize behavior fully operable by pointer, keyboard, and assistive technology.
- Preserve the existing desktop collapse and narrow-viewport drawer model.

**Non-Goals:**

- Resize Documents or the document reading column.
- Persist Review focus mode.
- Change review feed content, source navigation, or panel behavior above the app header.
- Add a general docking or window-management framework.

## Decisions

### Add one explicit Review focus control

Place a square icon button immediately before the **Review** heading.
Use outward-corner geometry for expand and inward-corner geometry for restore rather than a text label or ambiguous chevron.

The button uses `aria-pressed`, a title, and a dynamic accessible label:

- **Expand Review** when inactive.
- **Restore Review** when active.

Keep focus on the same button while toggling.
Escape restores the previous layout and returns focus to that button.

### Model expansion separately from panel visibility

Add `reviewExpanded` as transient page state and expose it as `data-review-expanded` on the layout.
Do not overwrite `documentsOpen`, `reviewOpen`, or their stored preferences when entering focus mode.

While expanded:

- Review remains open and fills the workspace below the header.
- Documents and reader become inert and visually hidden.
- Edge toggles and the resize separator are unavailable.
- The mobile backdrop is hidden because Review covers the complete workspace.

Restoring removes only the expanded state, so the prior open panels and reader geometry return exactly.
Reload always starts unexpanded.

### Resize through a semantic separator

Add a narrow hit target on Review's left edge with:

- `role="separator"`
- `aria-orientation="vertical"`
- `aria-controls="review"`
- `aria-valuemin`, `aria-valuemax`, and `aria-valuenow`
- `tabindex="0"` only when desktop resizing is available

The visible divider remains one pixel, while a wider transparent hit area makes dragging reliable.
During pointer drag, capture the pointer, disable text selection, and update one inline `--review-width` CSS custom property.
Commit the preference to local storage only when the drag ends.

Keyboard behavior:

- Left Arrow widens Review.
- Right Arrow narrows Review.
- Home removes the saved preference and restores the responsive default.

### Clamp width against the usable reader

Compute desktop bounds from the root font size, viewport width, current Documents width, and a minimum usable reader width.
Use approximately 17rem as the Review minimum, cap it near 55 percent of the viewport, and always reserve a practical reading column.

Keep the stored preferred width separate from the currently applied width.
On viewport or panel changes, clamp the applied width without erasing the user's larger saved preference, so it can return on a larger display.

The CSS default remains responsive when no preference exists.

### Keep resizing desktop-only and expansion responsive

Show the separator only at the existing desktop breakpoint while Review is open and not expanded.
Narrow layouts retain their current drawer dimensions when restored.
The expand control remains available in the drawer and makes Review fill the complete viewport width below the header.

### Avoid layout animation during drag

Do not animate the grid track while resizing.
Use the existing drawer transition only for narrow-viewport open and close behavior.
Reduced-motion behavior remains unchanged, and expansion does not introduce decorative motion.

## Risks / Trade-offs

- **A very wide Review panel can make documents unusable** -> Derive the maximum from remaining reader space and re-clamp after viewport or Documents changes.
- **Pointer dragging can select page text** -> Capture the pointer and apply a temporary resize state that disables selection.
- **Expansion can corrupt remembered panel preferences** -> Keep expanded state independent and never write panel visibility while toggling it.
- **A saved width can exceed a smaller display** -> Apply a viewport-specific clamp without discarding the stored preference.
- **Three panel controls can weaken header hierarchy** -> Keep expand before the title, retain collapse at the far edge, and use the same restrained icon-button styling.

## Migration Plan

No data or server migration is required.
Existing panel visibility preferences continue to work.
Users without a saved Review width receive the current responsive default.
