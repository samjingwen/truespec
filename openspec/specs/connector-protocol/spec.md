# connector-protocol Specification

## Purpose
Define the versioned local contract that coding-harness integrations use to pair a conversation, deliver TrueSpec feedback, and mirror the completed agent response.

## Requirements

### Requirement: Keep connectors independent from document sources

TrueSpec SHALL expose coding-harness delivery through a local connector protocol without requiring provider code or credentials in the browser or review core.
The initialized source adapter SHALL control only document discovery and validation.

#### Scenario: Pair a connector with an OpenSpec review

- **WHEN** a connector attaches to a repository initialized with either the OpenSpec or Markdown adapter
- **THEN** the destination identifies the conversation while project configuration independently identifies the documents
- **AND** changing connector implementations does not change document discovery behavior

### Requirement: Version and isolate connector attachment

Every connector operation SHALL use a supported protocol version and registered workspace.
Attachment SHALL bind an opaque destination, connector kind, and label to a trusted canonical root and repository-owned workspace ID.
It SHALL return protocol and workspace identity, canonical root, connection ID, review ID, and workspace-scoped browser URL.
Unsupported versions, duplicate workspace IDs, invalid project configuration, and mismatched roots SHALL fail before review state changes.

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

Shared-service attachment SHALL report one workspace-scoped browser URL using the selected loopback or verified tailnet origin.
The URL SHALL contain opaque workspace ID and review ID and SHALL NOT expose the canonical workspace path, connector destination, connection ID, control credential, or Tailscale user identity.
An old browser tab SHALL remain on its original workspace, review, and origin after another review begins.

#### Scenario: Open a paired review

- **WHEN** a connector attaches or reuses a workspace review in local mode
- **THEN** it reports the loopback URL with opaque workspace and review identity
- **AND** the URL reveals no filesystem path or coding-conversation identifier

#### Scenario: Open reviews for two projects

- **WHEN** two project integrations open their reported URLs against the shared listener
- **THEN** each browser resolves documents, comments, and replies only through its own workspace and review

#### Scenario: Open a paired review in tailnet mode

- **WHEN** a connector attaches or reuses a workspace review while tailnet mode is active
- **THEN** attachment returns the exact configured HTTPS tailnet origin with the workspace-scoped review path
- **AND** the connector reports that URL without reading or receiving the private local control credential

#### Scenario: Reattach after local service recovery

- **WHEN** a connector restarts the local service while persisted tailnet mode remains enabled
- **THEN** attachment verifies the owned Serve configuration before returning the tailnet URL
- **AND** it does not silently return an unverified remote origin

#### Scenario: Use an integration from the previous protocol

- **WHEN** an installed integration supports only the previous connector protocol
- **THEN** TrueSpec refuses attachment before changing review state
- **AND** it tells the user to refresh the integration with `truespec init --integration opencode --force`

### Requirement: Present review URLs in the coding conversation

An installed integration SHALL present the available review destinations as compact Markdown links in a normal conversation message.
It SHALL admit that message without requesting an agent response and SHALL NOT launch a browser automatically.

#### Scenario: Present a local review URL

- **WHEN** an integration opens a review in local mode
- **THEN** its conversation message contains exactly `[Local](<loopback-review-url>)`
- **AND** the label links to the canonical loopback review URL

#### Scenario: Present tailnet and local review URLs

- **WHEN** an integration opens a review in tailnet mode
- **THEN** its conversation message contains exactly `[Tailnet](<tailnet-review-url>) · [Local](<loopback-review-url>)`
- **AND** each label links to its canonical review URL

#### Scenario: Admit links without an agent turn

- **WHEN** an integration presents review links
- **THEN** OpenCode stores a normal user message without issuing a model request or creating an assistant response
- **AND** the message is available to future conversation context

#### Scenario: Present URLs without launching a browser

- **WHEN** an integration opens or resumes a review in local or tailnet mode
- **THEN** it leaves browser navigation under the user's control
- **AND** it does not invoke an operating-system browser launcher

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

### Requirement: Record delivery only after destination acceptance

A connector SHALL record delivery only after its destination accepts the batch.
The receipt SHALL identify the accepted harness message or turn.
Repeating the same receipt SHALL be idempotent.
A different receipt or stale lease SHALL NOT replace a recorded delivery.
A delivery receipt SHALL mean only that the destination accepted the batch, not that the agent completed it.

#### Scenario: Retry an ambiguous receipt write

- **WHEN** a connector repeats the same delivery receipt after an ambiguous result
- **THEN** TrueSpec accepts the retry without changing the recorded delivery

#### Scenario: Submit a conflicting receipt

- **WHEN** a connector submits another receipt for an already delivered batch
- **THEN** TrueSpec rejects it and preserves the first receipt

### Requirement: Support safe harness delivery retries

A connector SHOULD use a stable idempotency key derived from the batch ID when the harness supports one.
If the harness cannot deduplicate delivery, the connector SHALL check for an earlier accepted delivery before posting again after an ambiguous failure.
The connector SHALL persist enough binding information to resume delivery to the same destination after restart.

#### Scenario: Resume after connector restart

- **WHEN** the connector restarts with queued work
- **THEN** it resumes the existing connection and delivers the batch to the original destination
- **AND** it does not create a different review or coding conversation

### Requirement: Defer temporary failures

A connector SHALL be able to release a leased attempt with a retry delay after a temporary failure.
Deferring SHALL keep the batch queued without changing its immutable snapshot.

#### Scenario: Destination is temporarily unavailable

- **WHEN** the connector defers a valid leased batch with a retry delay
- **THEN** the batch becomes claimable again after that delay
- **AND** the next delivery attempt retains the same batch and comment IDs

### Requirement: Mirror one completed reply

After a delivered harness turn completes, a connector SHALL be able to store one Markdown reply using the harness's stable assistant-message ID.
Repeating the same reply ID and body SHALL be idempotent.
A different reply for the same batch SHALL be rejected.
The browser SHALL append the reply once to the chronological feed and link it to every batch comment.
Recording a reply SHALL mark the batch items handled.

#### Scenario: Store the completed harness response

- **WHEN** the connector records the completed response for a delivered batch
- **THEN** TrueSpec appends one linked reply after that batch's comments
- **AND** the linked feedback shows the agent-done state

#### Scenario: Retry response recording

- **WHEN** the connector repeats the same stable reply ID and Markdown body
- **THEN** TrueSpec accepts the retry without creating another feed entry

### Requirement: Drain an ended review without ending the coding chat

A final batch MAY report that the browser review has ended.
The connector SHALL continue retrieving queued batches until it receives the `ended` event.
The `ended` event SHALL terminate the TrueSpec delivery loop, not the coding conversation.

#### Scenario: Deliver a final batch

- **WHEN** the reviewer sends a final batch and ends the review
- **THEN** the connector delivers that batch with the ended state
- **AND** it continues until TrueSpec reports that no queued work remains
- **AND** the existing coding chat remains available

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
