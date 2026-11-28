# Multi-Project Runtime Delta

## Purpose

Let one TrueSpec service host concurrent reviews for multiple initialized repository checkouts while keeping source access and all review-domain state isolated inside each repository.

## ADDED Requirements

### Requirement: Identify one state-bearing workspace

Each initialized repository state database SHALL contain a stable random workspace ID representing that physical checkout and its local TrueSpec state.
The ID SHALL NOT represent a logical repository shared by every clone.

#### Scenario: Initialize a fresh checkout

- **WHEN** TrueSpec creates review state for a checkout that has no workspace ID
- **THEN** it generates one opaque collision-resistant ID inside that repository's untracked state database

#### Scenario: Move a state-bearing checkout

- **WHEN** the repository directory and its `truespec/.state/` contents move together
- **THEN** re-registration preserves the workspace ID and can update its validated canonical root

#### Scenario: Initialize another clone

- **WHEN** a fresh clone does not contain the first checkout's untracked state
- **THEN** it receives a different workspace ID

### Requirement: Refuse duplicate live workspace identities

One service registry SHALL bind a workspace ID to at most one available canonical root.
Copying local state into two simultaneously available checkouts SHALL require explicit operator resolution rather than silent rebinding.

#### Scenario: Register copied state at another live root

- **WHEN** a second available checkout presents a workspace ID already bound to another available root
- **THEN** registration fails and identifies a duplicate local-state condition
- **AND** neither root mapping is changed

#### Scenario: Rebind after a verified move

- **WHEN** the registered root is unavailable and another initialized root presents the same workspace ID and valid project state
- **THEN** TrueSpec can rebind the registry to the new canonical root after verifying the old location is not concurrently available

### Requirement: Keep review state repository-owned

Documents, comments, anchors, sessions, batches, connector bindings, delivery receipts, replies, and immutable snapshots SHALL remain in that workspace's `truespec/.state/review.sqlite3`.
The global service registry SHALL store only recoverable routing and availability metadata.

#### Scenario: Register a project

- **WHEN** an initialized repository registers with the service
- **THEN** the global registry stores its workspace ID, validated canonical root, configuration identity, and availability metadata
- **AND** it does not copy source Markdown or review-domain rows

#### Scenario: Lose the global registry

- **WHEN** global routing metadata is lost while repository state remains intact
- **THEN** reattaching an initialized repository can rebuild its registry mapping from the repository-owned workspace identity
- **AND** existing reviews and annotation history remain available

### Requirement: Validate project roots before every binding

The service SHALL accept project registration only from a trusted local client and SHALL canonicalize the root, verify the Git repository marker, load valid TrueSpec configuration, and reject symlink or source-boundary escapes.
Browser requests SHALL never choose or submit filesystem paths.

#### Scenario: Attach an initialized repository

- **WHEN** a trusted local client registers a canonical initialized repository
- **THEN** the service validates its configuration and source before publishing a workspace handle

#### Scenario: Submit a path from the browser

- **WHEN** a browser request includes an arbitrary repository or document filesystem path as a routing instruction
- **THEN** the service rejects it and resolves resources only from the registered workspace and discovered document identities

### Requirement: Isolate concurrent project contexts

The service SHALL resolve every document, review, comment, connector, and refresh operation through one workspace context.
Different workspaces SHALL be able to keep open reviews concurrently without sharing state, source objects, or connector delivery.

#### Scenario: Review two projects concurrently

- **WHEN** two initialized repositories attach to one service and open reviews
- **THEN** each browser URL and connector binding resolves through its own workspace ID and review ID
- **AND** comments, source refreshes, batches, and replies from one workspace never appear in the other

#### Scenario: Reuse one project's open review

- **WHEN** the same integration destination reattaches to the same workspace while its review is open
- **THEN** TrueSpec reuses that project's connection and review without affecting another workspace

#### Scenario: Open reviews in separate projects

- **WHEN** each of two projects has one open review
- **THEN** both reviews remain valid because the one-open-review invariant applies independently to each project database

### Requirement: Load and retire project contexts safely

The service SHALL load registered project contexts lazily and SHALL revalidate project availability and configuration before serving them after restart or cache eviction.
Removing an in-memory context SHALL NOT delete or reset repository state.

#### Scenario: Restart the shared service

- **WHEN** the daemon restarts with multiple registered workspaces
- **THEN** it restores routing metadata without eagerly reading every source tree
- **AND** each workspace is validated when a browser or connector next accesses it

#### Scenario: Registered root becomes unavailable

- **WHEN** a registered repository is missing, relocating, or no longer valid
- **THEN** requests for that workspace report it as unavailable without falling back to another root
- **AND** other project reviews continue operating
