# Connector Protocol Delta

## MODIFIED Requirements

### Requirement: Keep connectors independent from document sources

TrueSpec SHALL expose coding-harness delivery through a local connector protocol without requiring provider code or credentials in the browser or review core.
The initialized source adapter SHALL control only document discovery and validation.

#### Scenario: Pair a connector with an OpenSpec review

- **WHEN** a connector attaches to a repository initialized with either the OpenSpec or Markdown adapter
- **THEN** the destination identifies the conversation while project configuration independently identifies the documents
- **AND** changing connector implementations does not change document discovery behavior

### Requirement: Version and isolate connector attachment

Every connector operation SHALL require a supported protocol version.
Attaching SHALL accept an opaque stable destination, connector kind, and display label, then resolve the initialized project source.
It SHALL return the protocol version, canonical workspace, connection ID, and review ID.
An unsupported version or invalid project configuration SHALL fail without changing review state.

#### Scenario: Reattach the same destination

- **WHEN** the same destination attaches again while its repository review is open
- **THEN** TrueSpec reuses the existing connection and review

#### Scenario: Another destination tries to attach

- **WHEN** a different destination tries to attach while the repository review is open
- **THEN** TrueSpec rejects the attachment without redirecting existing or pending feedback

#### Scenario: Pair after manual submission

- **WHEN** the first initialized integration attaches to an existing open review with incomplete connector-free batches
- **THEN** TrueSpec assigns their outstanding items to that connection in submission order
- **AND** it preserves immutable snapshots and completed items while clearing obsolete manual leases

### Requirement: Lease immutable batches in order

Connector retrieval SHALL return a versioned envelope with a `batch`, `idle`, or `ended` event.
A batch event SHALL include the batch ID, lease token, lease expiry, attempt number, session-ended state, and immutable comment snapshots.
TrueSpec SHALL deliver connector batches in submission order to their paired connection.
A connector lease SHALL prevent another connection from claiming the same batch.

#### Scenario: Claim the next paired batch

- **WHEN** a connection retrieves pending work
- **THEN** it receives the oldest undelivered batch for that connection with an expiring lease
- **AND** another claimant cannot receive that batch while the lease remains valid

#### Scenario: Retry after an interrupted claim

- **WHEN** a connector stops without recording delivery or deferring the attempt
- **THEN** the lease eventually expires and a later attempt can claim the same immutable batch
