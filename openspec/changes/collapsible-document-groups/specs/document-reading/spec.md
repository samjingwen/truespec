# Document Reading Delta

## ADDED Requirements

### Requirement: Provide structured document navigation

Each document-source adapter SHALL provide an ordered navigation path made of stable segment IDs, display labels, and default disclosure states.
The browser SHALL render that structure without parsing separators or source-specific naming conventions from labels.

#### Scenario: Structure current OpenSpec capabilities

- **WHEN** current specifications include `delivery/flux-reconciliation` and `delivery/image-and-release`
- **THEN** the OpenSpec adapter places both documents under **Current specifications** and nested **delivery** segments
- **AND** their leaf labels are **flux-reconciliation** and **image-and-release**
- **AND** no directory label contains a trailing slash

#### Scenario: Structure nested OpenSpec artifacts

- **WHEN** a change, archive, delta, unfamiliar artifact, or custom-schema artifact contains nested directories
- **THEN** the OpenSpec adapter emits one navigation segment for every semantic or real directory level
- **AND** it preserves the artifact's real source path

#### Scenario: Structure plain Markdown directories

- **WHEN** a Markdown source contains documents at arbitrary directory depths
- **THEN** the Markdown adapter emits **Documents** followed by one navigation segment for each directory
- **AND** the final document label contains only its filename

### Requirement: Collapse document navigation groups

The Documents sidebar SHALL render every navigation segment as a keyboard-accessible disclosure.
Nested folders SHALL remain independently collapsible at arbitrary depth.
The browser SHALL remember each disclosure's state locally.

#### Scenario: Open the sidebar for the first time

- **WHEN** no saved disclosure state exists
- **THEN** current specifications, active changes, and plain Markdown directory groups are open
- **AND** archived changes are closed
- **AND** nested directories and `delta` folders are open

#### Scenario: Collapse a document group

- **WHEN** a reviewer toggles any navigation segment
- **THEN** only that segment's descendant folders and document links are hidden or shown
- **AND** the document currently displayed in the reader does not change

#### Scenario: Persist independent group state

- **WHEN** a reviewer changes disclosure states at any depth and reloads the page
- **THEN** each disclosure restores its own saved state independently

#### Scenario: Reveal the active document

- **WHEN** navigation or a refreshed URL selects a document inside one or more closed folders
- **THEN** the sidebar opens every ancestor disclosure needed to reveal the active document link
- **AND** the revealed state becomes the saved state for those ancestors

#### Scenario: Use disclosures in a narrow drawer

- **WHEN** the Documents panel is shown on a narrow viewport
- **THEN** every hierarchy level retains the same disclosure, keyboard, and persistence behavior as desktop
