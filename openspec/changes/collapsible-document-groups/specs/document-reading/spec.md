# Document Reading Delta

## ADDED Requirements

### Requirement: Collapse document navigation groups

The Documents sidebar SHALL render every top-level source group as a keyboard-accessible disclosure.
Nested document folders SHALL remain independently collapsible.
The browser SHALL remember each disclosure's state locally.

#### Scenario: Open the sidebar for the first time

- **WHEN** no saved disclosure state exists
- **THEN** current specifications, active changes, and plain Markdown directory groups are open
- **AND** archived changes are closed
- **AND** nested `delta/` folders are open

#### Scenario: Collapse a document group

- **WHEN** a reviewer toggles a top-level group
- **THEN** only that group's document links and nested folders are hidden or shown
- **AND** the document currently displayed in the reader does not change

#### Scenario: Persist independent group state

- **WHEN** a reviewer changes top-level or nested disclosure states and reloads the page
- **THEN** each disclosure restores its own saved state independently

#### Scenario: Reveal the active document

- **WHEN** navigation or a refreshed URL selects a document inside a closed group or nested folder
- **THEN** the sidebar opens every ancestor disclosure needed to reveal the active document link
- **AND** the revealed state becomes the saved state for those ancestors

#### Scenario: Use disclosures in a narrow drawer

- **WHEN** the Documents panel is shown on a narrow viewport
- **THEN** top-level and nested groups retain the same disclosure, keyboard, and persistence behavior as desktop
