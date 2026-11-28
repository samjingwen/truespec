# Document Reading Delta

## MODIFIED Requirements

### Requirement: Initialized reader startup

An installed integration SHALL obtain a workspace-scoped loopback reader URL from the shared service using the initialized project's versioned source configuration.
The shared reader SHALL NOT require Node.js, OpenSpec, or the selected coding harness after the workspace context is attached.

#### Scenario: Open an initialized OpenSpec review

- **WHEN** a user invokes a selected integration in a project configured with the OpenSpec adapter
- **THEN** the shared service validates or loads that workspace context and displays its configured OpenSpec documents

#### Scenario: Open an initialized Markdown review

- **WHEN** a user invokes a selected integration in a project configured with the Markdown adapter
- **THEN** the same shared reader and annotation controls display Markdown under that workspace's configured source path

#### Scenario: Open two initialized projects

- **WHEN** integrations in two initialized repositories open reviews concurrently
- **THEN** one listener serves separate workspace-scoped catalogs and review boards
- **AND** navigation, source refresh, browser history, and old tabs remain bound to their original workspace

#### Scenario: Invoke TrueSpec before initialization

- **WHEN** an integration tries to open a review in a repository without valid TrueSpec project configuration
- **THEN** it does not register or open that workspace and tells the user to run `truespec init` from the repository root

#### Scenario: Workspace becomes unavailable

- **WHEN** an existing browser tab requests a registered workspace whose root is missing or no longer validates
- **THEN** the reader reports that workspace as unavailable
- **AND** it does not substitute another workspace or expose its documents
