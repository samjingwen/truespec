# Document Reading Delta

## MODIFIED Requirements

### Requirement: Initialized reader startup

An installed integration SHALL obtain a workspace-scoped reader URL from the shared service using the initialized project's versioned source configuration.
The URL SHALL use loopback by default and the configured private tailnet HTTPS origin while tailnet mode is active.
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

#### Scenario: Open a review through the tailnet origin

- **WHEN** tailnet mode is active and an initialized integration opens a review
- **THEN** the reported browser URL uses the verified private HTTPS tailnet origin and the same opaque workspace and review identity
- **AND** durable document links, refresh, history, and direct navigation remain on that workspace-scoped tailnet path

#### Scenario: Keep a local review available in tailnet mode

- **WHEN** tailnet mode is active and the loopback URL is opened on the host machine
- **THEN** the local browser can use the same workspace and review without creating another session

#### Scenario: Invoke TrueSpec before initialization

- **WHEN** an integration tries to open a review in a repository without valid TrueSpec project configuration
- **THEN** it does not register or open that workspace and tells the user to run `truespec init` from the repository root

#### Scenario: Workspace becomes unavailable

- **WHEN** an existing browser tab requests a registered workspace whose root is missing or no longer validates
- **THEN** the reader reports that workspace as unavailable
- **AND** it does not substitute another workspace or expose its documents
