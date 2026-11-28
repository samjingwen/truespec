# service-lifecycle Specification

## Purpose
Provide one discoverable, authenticated, user-scoped TrueSpec background service that the CLI and installed integrations can start, inspect, reuse, and stop safely.

## Requirements

### Requirement: Expose explicit service lifecycle commands

Normal TrueSpec help SHALL expose `up`, `status`, and `down` alongside `init`.
Lifecycle commands SHALL manage one user-scoped service independently from the current repository.

#### Scenario: Inspect public help

- **WHEN** a user runs `truespec --help`
- **THEN** the output lists `init`, `up`, `status`, and `down`
- **AND** it does not expose the daemon's internal run entrypoint or connector transport commands

#### Scenario: Start from outside a repository

- **WHEN** a user runs `truespec up` from a directory that is not an initialized repository
- **THEN** TrueSpec starts or reuses the user-scoped service without requiring project configuration

### Requirement: Start one compatible service idempotently

`truespec up` SHALL return only after a compatible service reports authenticated readiness.
Concurrent startup attempts SHALL produce one owner, and repeating `up` against that owner SHALL not start another service.

#### Scenario: Start while stopped

- **WHEN** no healthy service is registered and the user runs `truespec up`
- **THEN** TrueSpec starts one detached service and waits until its authenticated health endpoint is ready

#### Scenario: Repeat startup

- **WHEN** the compatible service is already healthy and the user runs `truespec up`
- **THEN** the command reports the existing service as running
- **AND** the instance identity and process remain unchanged

#### Scenario: Race startup commands

- **WHEN** multiple clients try to start the service concurrently
- **THEN** one process acquires ownership and publishes the registration
- **AND** losing contenders exit without replacing or disrupting that owner

### Requirement: Discover service state through authenticated health

Service discovery SHALL use a private atomic registration record and an authenticated health request.
The registration record alone SHALL NOT be treated as proof that the service is running or compatible.

#### Scenario: Report a healthy service

- **WHEN** `truespec status` reads a registration whose authenticated service reports the expected protocol and version
- **THEN** it reports the service as running with safe health, version, endpoint, project count, and an open-review count when all repository state is loaded
- **AND** it reports the open-review count as explicitly unknown rather than zero while lazy repository state remains unloaded
- **AND** it omits credentials and private project paths from default output

#### Scenario: Report a stopped service

- **WHEN** no registration exists and no service owns the configured endpoint
- **THEN** `truespec status` reports the service as stopped without starting it

#### Scenario: Reject an incompatible owner

- **WHEN** a registered endpoint answers with an unsupported health protocol or incompatible binary version
- **THEN** TrueSpec reports the incompatibility and an explicit recovery action
- **AND** it does not silently adopt, replace, or signal that process

### Requirement: Recover stale registration without adopting another process

TrueSpec SHALL compare service instance identity, process identity, endpoint, protocol, and version before repairing or removing runtime registration.
An unrelated process SHALL never be treated as the TrueSpec service merely because it reuses a PID or port.

#### Scenario: Registration points to no responsive owner

- **WHEN** repeated bounded health checks show that the registered instance is absent
- **THEN** `up` can retire only that matching stale registration and start a new owner

#### Scenario: Another process owns the endpoint

- **WHEN** the configured endpoint is occupied but does not prove the registered TrueSpec identity
- **THEN** startup fails with an actionable collision error
- **AND** TrueSpec does not signal or proxy to the unrelated process

### Requirement: Stop the matching service safely

`truespec down` SHALL request graceful shutdown of the authenticated registered service, wait within a bounded deadline, and verify that the matching owner stopped.
Stopping the daemon SHALL preserve every repository's configuration and review database.

#### Scenario: Stop a healthy service

- **WHEN** the user runs `truespec down` while a matching service is healthy
- **THEN** TrueSpec requests graceful drain, waits for that process to exit, and removes only its matching registration
- **AND** a later `up` can reopen registered project state

#### Scenario: Stop while already stopped

- **WHEN** no service is registered and the user runs `truespec down`
- **THEN** the command succeeds idempotently and reports that the service is already stopped

#### Scenario: Registration changes during shutdown

- **WHEN** the registration changes to another instance while shutdown is in progress
- **THEN** the client does not remove or signal the replacement instance

### Requirement: Keep global operational state private and bounded

Service discovery credentials, routing metadata, and logs SHALL be stored in owner-only global directories.
Logs SHALL be bounded and SHALL NOT contain source text, review bodies, connector destinations, authentication credentials, or complete browser URLs.

#### Scenario: Inspect global service files

- **WHEN** the service creates registration, registry, or log files
- **THEN** their directories and sensitive files are restricted to the current user
- **AND** project review-domain records remain absent from global storage

#### Scenario: Emit an operational failure

- **WHEN** startup, registration, project loading, or shutdown fails
- **THEN** logs record bounded operational identifiers and recovery context
- **AND** they redact project paths and review content
