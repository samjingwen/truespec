# project-initialization Specification

## Purpose

Provide one repository-root workflow that configures TrueSpec's document source and installs any selected coding-harness integrations safely.

## Requirements

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

### Requirement: Handle unsupported local review state

Initialization SHALL refuse unsupported review-database versions without modifying them.
An explicit state reset SHALL discard only validated repository-local review state and create a fresh workspace identity.

#### Scenario: Encounter an unsupported review database

- **WHEN** initialization finds a review database version the current binary does not support
- **THEN** it refuses registration without modifying or deleting that database
- **AND** it tells the user that `truespec init --reset-state` can discard local review state and start fresh

#### Scenario: Explicitly reset local review state

- **WHEN** the user runs `truespec init --reset-state`
- **THEN** TrueSpec warns that drafts, reviews, batches, replies, and connector bindings will be permanently deleted
- **AND** it gracefully stops and restarts the shared local service around deletion
- **AND** it removes only a validated repository-local `truespec/.state/` directory
- **AND** it creates a fresh database and workspace ID while preserving source Markdown and project configuration
- **AND** if old-state cleanup is incomplete, the fresh state remains active and the command reports the retained quarantine

### Requirement: Configure a registered document source

Initialization SHALL select a source adapter from the built-in source registry and store its ID and workspace-relative source path.
OpenSpec and plain Markdown SHALL remain supported through the same review core.

#### Scenario: Detect OpenSpec

- **WHEN** the repository contains an `openspec/` directory and the user does not pass a source option
- **THEN** initialization selects the OpenSpec adapter with `openspec/` as its source path

#### Scenario: Default to repository Markdown

- **WHEN** the repository has no `openspec/` directory and the user does not pass a source option
- **THEN** initialization selects the Markdown adapter with `.` as its source path

#### Scenario: Override source detection

- **WHEN** the user passes `--source <adapter> --source-path <path>`
- **THEN** initialization validates that registered adapter and directory and stores the explicit workspace-relative configuration

#### Scenario: Reject an unsafe source path

- **WHEN** the configured source path is missing, leaves the repository, resolves through a symlink outside it, or points into excluded TrueSpec state
- **THEN** initialization fails without writing project configuration or integration artifacts

### Requirement: Select multiple coding integrations

Initialization SHALL select integrations from a built-in integration registry.
Interactive initialization SHALL provide a multi-select, while non-interactive initialization SHALL support repeatable `--integration` and `--all`.

#### Scenario: Choose integrations interactively

- **WHEN** `truespec init` runs in an interactive terminal without integration flags
- **THEN** it presents every registered integration in one multi-select and installs the confirmed selections

#### Scenario: Choose integrations non-interactively

- **WHEN** the user repeats `--integration` with registered IDs
- **THEN** initialization installs every distinct selected integration

#### Scenario: Select every integration

- **WHEN** the user passes `--all`
- **THEN** initialization selects every registered integration exactly once

#### Scenario: Omit a required non-interactive selection

- **WHEN** initialization runs without an interactive terminal and without `--integration` or `--all`
- **THEN** it lists available integrations, prints an exact non-interactive command, and makes no changes

### Requirement: Preflight integration installation

TrueSpec SHALL build and validate one installation plan for all selected integrations before writing any artifact.
A differing existing artifact SHALL block the full plan unless `--force` is explicit.

#### Scenario: One selected integration conflicts

- **WHEN** any selected integration would replace a differing plugin, skill, or configuration file and `--force` is absent
- **THEN** initialization reports every conflict and writes none of the selected integration artifacts

#### Scenario: Repair a partial installation

- **WHEN** matching artifacts already exist and other expected artifacts are missing
- **THEN** initialization preserves the matching files and atomically installs the missing files

#### Scenario: Force an integration update

- **WHEN** the user passes `--force` and selected integration artifacts differ
- **THEN** TrueSpec atomically replaces only those selected artifacts with its generated versions

### Requirement: Keep the public CLI focused

Normal TrueSpec help SHALL expose `init`, `up`, `status`, `down`, help, and version as the complete user command surface.
Daemon internals, integration transport, and browser API operations SHALL remain versioned but hidden from normal user help.

#### Scenario: Inspect command help

- **WHEN** a user runs `truespec --help`
- **THEN** the output presents repository initialization and shared-service lifecycle commands
- **AND** it does not advertise daemon internals, viewer reset, manual inbox, relink, setup, or connector transport commands
