# Proposal

## Why

TrueSpec currently exposes separate viewer, setup, manual inbox, and connector workflows even though the intended product experience is one initialized repository reviewed through an existing coding harness.
Consolidating setup and source selection into `truespec init` makes the common path clear while preserving OpenSpec and plain Markdown through extensible adapters.

## What Changes

- Add repository-root `truespec init` with source detection, versioned project configuration, and an interactive multi-select for coding integrations.
- Support repeatable `--integration`, `--all`, `--source`, and adapter-friendly `--source-path` options for non-interactive initialization.
- Keep OpenSpec and plain Markdown as independent built-in source adapters selected through a compile-time registry.
- Introduce an integration registry whose adapters contribute installation artifacts, bundled skills, and connector behavior.
- Install a portable `truespec-review` skill with each supported integration and shorten delivered feedback to a skill invocation plus immutable batch data.
- Make `/truespec-review` read the initialized source configuration instead of hardcoding OpenSpec.
- Keep only `init`, help, and version in the public CLI while moving viewer and delivery operations behind a hidden versioned connector boundary.
- Remove the standalone browser handoff and manual agent loop because every browser review starts from a paired integration.
- Preserve existing SQLite review state and adopt outstanding manual batches into the first initialized integration without rewriting their immutable snapshots.
- **BREAKING** Remove the public `serve`, `setup`, `listen`, `ack`, `complete`, `reply`, and `relink` command surface.

## Capabilities

### New Capabilities

- `project-initialization`: Initialize a repository, configure its document source, select multiple coding integrations, and apply installation changes safely.

### Modified Capabilities

- `document-reading`: Start readers from initialized project configuration while retaining independent OpenSpec and Markdown adapters.
- `document-annotations`: Preserve missing-file evidence without exposing the removed public relink command.
- `review-handoff`: Require paired reviews, deliver feedback through an installed skill, and remove the standalone manual handoff.
- `connector-protocol`: Read source configuration during attachment, adopt legacy pending batches, and keep transport commands internal to integrations.

## Impact

- Affects the public CLI, project configuration, source and integration registries, setup installer, OpenCode plugin, bundled Agent Skill, browser UI, SQLite batch attachment, README, and all end-to-end tests.
- Keeps the current review database schema and loopback browser architecture.
- Adds a terminal multi-select dependency but no runtime dependency on OpenSpec, OpenCode, Node.js, or external agent services.
