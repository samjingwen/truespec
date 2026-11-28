# Project Initialization Delta

## MODIFIED Requirements

### Requirement: Initialize at the repository root

`truespec init` SHALL initialize only the current repository root and SHALL write a versioned project configuration under `truespec/config/`.
It SHALL complete project and integration preflight, ensure a compatible background service is healthy, apply the initialization plan, and register the resulting workspace.
Initialization SHALL preserve source Markdown and existing review state.

#### Scenario: Initialize a repository

- **WHEN** a user runs `truespec init` from a directory containing a Git repository marker
- **THEN** TrueSpec writes or confirms the project configuration for that canonical repository root
- **AND** it leaves existing reviewed documents and review history unchanged
- **AND** it registers the repository-owned workspace ID with the healthy shared service

#### Scenario: Initialize while the service is stopped

- **WHEN** initialization preflight succeeds and no compatible service is running
- **THEN** TrueSpec starts the user-scoped service and waits for authenticated readiness before applying project files

#### Scenario: Service startup fails

- **WHEN** initialization preflight succeeds but the shared service cannot become healthy
- **THEN** initialization reports the service failure and recovery action
- **AND** it does not apply project configuration or integration artifacts

#### Scenario: Workspace registration fails after file installation

- **WHEN** project files are applied but workspace registration cannot complete
- **THEN** initialization reports that the repository is initialized but not registered
- **AND** repeating `truespec init` can repair registration without duplicating or rewriting matching files

#### Scenario: Run init outside the root

- **WHEN** a user runs `truespec init` from a nested directory or a directory without a Git repository marker
- **THEN** TrueSpec makes no changes, does not start the service, and tells the user to run the command from the repository root

#### Scenario: Repeat initialization

- **WHEN** the repository is already initialized and registered with the requested source and integrations
- **THEN** `truespec init` reuses the healthy service and succeeds without duplicating configuration, installation files, or workspace records

### Requirement: Keep the public CLI focused

Normal TrueSpec help SHALL expose `init`, `up`, `status`, `down`, help, and version as the complete user command surface.
Daemon internals, integration transport, and browser API operations SHALL remain versioned but hidden from normal user help.

#### Scenario: Inspect command help

- **WHEN** a user runs `truespec --help`
- **THEN** the output presents repository initialization and shared-service lifecycle commands
- **AND** it does not advertise daemon internals, viewer reset, manual inbox, relink, setup, or connector transport commands
