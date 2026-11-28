# Proposal

## Why

TrueSpec has no released legacy review database that needs in-place migration.
Automatically migrating development-era schemas adds lock complexity and failure risk without preserving supported user data.
Users still need an explicit, safe way to discard incompatible local review state and create a fresh workspace identity.

## What Changes

- Refuse review database versions older than the current schema without modifying or deleting them.
- Add `truespec init --reset-state` as an explicit destructive option that removes only validated repository-local review state.
- Forget the previous workspace route before reset, then create and register a fresh database and workspace ID.
- Keep `--force` limited to generated integration artifacts.
- Document the permanent loss of drafts, reviews, batches, replies, and connector bindings.

## Capabilities

### Modified Capabilities

- `project-initialization`: Define unsupported database refusal and explicit local state reset behavior.

## Impact

- Affects initialization flags, repository state handling, workspace routing cleanup, review database bootstrap, tests, and upgrade guidance.
- Does not delete source Markdown, shared project configuration, integration artifacts, or global state for other repositories.
