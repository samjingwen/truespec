# Document Reading Delta

## MODIFIED Requirements

### Requirement: Responsive review workspace

The viewer SHALL let reviewers independently collapse Documents and Review on desktop.
It SHALL let reviewers temporarily expand Review across the complete workspace and adjust the open Review width on desktop.
On smaller viewports, each panel SHALL remain an overlay drawer.
Panel controls SHALL remain keyboard accessible and expose their current state.

#### Scenario: Collapse desktop panels

- **WHEN** a reviewer collapses either desktop panel
- **THEN** the reader expands into the released space and hidden panel controls cannot receive focus
- **AND** the browser remembers that panel preference

#### Scenario: Use a panel on a narrow viewport

- **WHEN** a reviewer opens Documents or Review on a narrow viewport
- **THEN** it overlays the reader, closes the other drawer, and can be dismissed by its toggle, the backdrop, or Escape
- **AND** focus returns to the opening toggle after light dismissal

#### Scenario: Expand and restore Review

- **WHEN** a reviewer activates the expand control immediately before the **Review** heading
- **THEN** Review fills the complete workspace below the app header while Documents and the reader are hidden and inert
- **AND** the control identifies the expanded state and changes its accessible action to **Restore Review**
- **AND** activating it again or pressing Escape restores the exact prior panel layout and width
- **AND** expanded state is not persisted across reloads

#### Scenario: Resize Review with a pointer

- **WHEN** a reviewer drags the separator on Review's left edge on a desktop viewport
- **THEN** Review follows the pointer between defined minimum and maximum widths without making the document reader unusable
- **AND** the chosen width is stored for later page loads
- **AND** text selection and layout transitions do not interfere with the drag

#### Scenario: Resize Review with a keyboard

- **WHEN** the desktop separator has keyboard focus
- **THEN** Left Arrow widens Review, Right Arrow narrows it, and Home restores the default width
- **AND** the separator exposes its orientation, current value, minimum, and maximum to assistive technology

#### Scenario: Adapt a saved width

- **WHEN** a saved Review width no longer fits after the viewport changes
- **THEN** the viewer clamps it to the current safe range
- **AND** the resize separator is unavailable while Review is collapsed, expanded, or shown as a narrow-viewport drawer
- **AND** expanding Review on a narrow viewport fills the available workspace
