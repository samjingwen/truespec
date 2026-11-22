# Project Initialization

## Purpose

Provide one repository-root workflow that configures TrueSpec's document source and installs any selected coding-harness integrations safely.

## ADDED Requirements

### Requirement: Initialize at the repository root

`truespec init` SHALL initialize only the current repository root and SHALL write a versioned project configuration under `truespec/config/`.
Initialization SHALL preserve source Markdown and existing review state.

#### Scenario: Initialize a repository

- **WHEN** a user runs `truespec init` from a directory containing a Git repository marker
- **THEN** TrueSpec writes or confirms the project configuration for that canonical repository root
- **AND** it leaves `truespec/.state/` and reviewed documents unchanged

#### Scenario: Run init outside the root

- **WHEN** a user runs `truespec init` from a nested directory or a directory without a Git repository marker
- **THEN** TrueSpec makes no changes and tells the user to run the command from the repository root

#### Scenario: Repeat initialization

- **WHEN** the repository is already initialized with the requested source and integrations
- **THEN** `truespec init` succeeds without duplicating configuration or installation files

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

Normal TrueSpec help SHALL expose `init`, help, and version as the complete user command surface.
Integration transport and viewer operations SHALL remain versioned but hidden from normal user help.

#### Scenario: Inspect command help

- **WHEN** a user runs `truespec --help`
- **THEN** the output presents initialization as the only product command
- **AND** it does not advertise viewer, manual inbox, relink, setup, or connector transport commands
