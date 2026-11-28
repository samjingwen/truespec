# Connector Protocol Delta

## ADDED Requirements

### Requirement: Use the shared service for connector transport

Installed integrations SHALL discover and authenticate to the user-scoped TrueSpec service instead of starting one viewer process per repository.
Connector operations SHALL ensure a compatible service is healthy or return an actionable lifecycle error.

#### Scenario: Start a review after initialization

- **WHEN** an initialized integration invokes review from its bound repository and the service is healthy
- **THEN** it attaches through that service and receives the repository's workspace-scoped review binding and browser URL
- **AND** it does not spawn another long-lived viewer process

#### Scenario: Service stopped after initialization

- **WHEN** an integration invokes review after the shared service stopped
- **THEN** the connector client starts or reuses one compatible service through the same checked lifecycle used by `truespec up`
- **AND** it continues only after authenticated readiness

#### Scenario: Service protocol is incompatible

- **WHEN** an integration finds a service whose connector or health protocol is unsupported
- **THEN** it refuses attachment and tells the user to refresh the installed integration or restart with a compatible TrueSpec binary
- **AND** it does not deliver against an unverified endpoint

## MODIFIED Requirements

### Requirement: Version and isolate connector attachment

Every connector operation SHALL require a supported protocol version and a registered workspace context.
Attaching SHALL accept an opaque stable destination, connector kind, and display label, then resolve the initialized project through its trusted canonical root and repository-owned workspace ID.
It SHALL return the protocol version, workspace ID, canonical workspace, connection ID, review ID, and workspace-scoped browser URL.
An unsupported version, duplicate workspace identity, invalid project configuration, or mismatched root SHALL fail without changing review state.

#### Scenario: Reattach the same destination

- **WHEN** the same destination attaches again to the same workspace while its repository review is open
- **THEN** TrueSpec reuses the existing connection, review, and workspace-scoped URL

#### Scenario: Another destination tries to attach

- **WHEN** a different destination tries to attach while that repository review is open
- **THEN** TrueSpec rejects the attachment without redirecting existing or pending work

#### Scenario: Attach concurrent destinations in separate projects

- **WHEN** integrations in two registered workspaces attach their own destinations
- **THEN** each receives an independent connection and review for its workspace
- **AND** neither binding can lease or complete the other workspace's batches

#### Scenario: Pair after manual submission

- **WHEN** the first initialized integration attaches to an existing open review with incomplete connector-free batches
- **THEN** TrueSpec assigns their outstanding items to that workspace connection in submission order
- **AND** it preserves immutable snapshots and completed items while clearing obsolete manual leases

### Requirement: Keep browser identity separate from connector identity

Shared-service attachment SHALL report a browser URL scoped by opaque workspace ID and review ID.
The URL SHALL NOT expose the canonical workspace path, connector destination, or connection ID.
An old browser tab SHALL remain on its original workspace and review after another review begins in the same or another project.

#### Scenario: Open a paired review

- **WHEN** a connector attaches or reuses a workspace review
- **THEN** it opens the reported URL with opaque workspace and review identity
- **AND** the URL reveals no filesystem path or coding-conversation identifier

#### Scenario: Open reviews for two projects

- **WHEN** two project integrations open their reported URLs against the shared listener
- **THEN** each browser resolves documents, comments, and replies only through its own workspace and review
