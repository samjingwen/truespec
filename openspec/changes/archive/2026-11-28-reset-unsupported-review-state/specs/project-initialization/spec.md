# Project Initialization Delta

## ADDED Requirements

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
