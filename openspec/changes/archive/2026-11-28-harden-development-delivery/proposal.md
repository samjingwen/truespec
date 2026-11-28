# Proposal

## Why

TrueSpec's repository guidance defines verification commands but does not define an isolated, reviewable path from a requested change through a synchronized local `master`.
The public repository also lacks hosted checks and branch protection, so direct updates can bypass the same gates expected during local development.

## What Changes

- Require every repository edit to occur in a dedicated Worktrunk worktree.
- Define implementation, verification, Hardlook review, pull request, GitHub merge, local `master` synchronization, and coding-harness session-return steps in `AGENTS.md`.
- Add read-only GitHub Actions jobs for the ordinary quality gate and the browser plus OpenCode integration gate.
- Protect `master` with pull-request-only updates, strict required checks, administrator enforcement, resolved conversations, and force-push and deletion prevention.
- Keep release and deployment authority outside this development workflow.

## Capabilities

### New Capabilities

None.
This change affects repository development tooling and policy rather than TrueSpec product behavior.

### Modified Capabilities

None.

## Impact

The change affects `AGENTS.md`, the public development guide, GitHub Actions, and repository branch-protection settings.
It adds development-only Node package downloads in CI without adding runtime dependencies to the TrueSpec binary.
