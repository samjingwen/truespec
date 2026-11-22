# Document Reading Delta

## ADDED Requirements

### Requirement: Initialized reader startup

An installed integration SHALL start the loopback reader from the initialized project's versioned source configuration.
The reader SHALL NOT require Node.js, OpenSpec, or the selected coding harness after the integration starts it.

#### Scenario: Open an initialized OpenSpec review

- **WHEN** a user invokes a selected integration in a project configured with the OpenSpec adapter
- **THEN** TrueSpec starts or reuses the loopback reader and displays that source path's OpenSpec documents

#### Scenario: Open an initialized Markdown review

- **WHEN** a user invokes a selected integration in a project configured with the Markdown adapter
- **THEN** the same reader and annotation controls display Markdown under the configured source path

#### Scenario: Invoke TrueSpec before initialization

- **WHEN** an integration tries to start a review in a repository without valid TrueSpec project configuration
- **THEN** it does not start the reader and tells the user to run `truespec init` from the repository root

## REMOVED Requirements

### Requirement: Local reader startup

**Reason**: The product now starts readers only through initialized coding integrations, so the direct public `truespec serve` workflow and testing reset flag are no longer part of the user contract.

**Migration**: Run `truespec init` at the repository root, select the source adapter and integrations, then start the review from a selected coding harness.
