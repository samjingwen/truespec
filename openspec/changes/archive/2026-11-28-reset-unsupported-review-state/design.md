# Design

## Context

The shared service originally planned append-only migration from earlier development database versions.
The product will instead treat the current review schema as the first supported state format.

## Decisions

### Refuse unsupported versions without mutation

Opening a non-current database returns an actionable error that names `truespec init --reset-state`.
TrueSpec does not run migration SQL, change `PRAGMA user_version`, or delete state during ordinary initialization.

### Keep destructive intent separate from integration replacement

`--force` continues to replace differing generated integration files only.
`--reset-state` is a dedicated option whose name and warning describe permanent local review-state loss.

### Drain the shared service before deleting repository state

The CLI acquires a maintenance lock before gracefully stopping the shared service, and automatic connector startup respects that lock.
While continuously holding maintenance ownership and the offline service-owner lock, it removes the canonical root's stale recoverable registry entry and `truespec/.state/`, so no service can restart or retain an active database handle during deletion.
It then deletes state, restarts the service, creates a new database and workspace ID, and registers the root again.
Recursive deletion of quarantined old state is post-commit cleanup; if it is incomplete, fresh state remains authoritative and the CLI reports the retained quarantine instead of restoring a partially deleted directory.

### Restrict deletion to validated local state

Reset canonicalizes the repository root, rejects symlinked or non-directory state paths, and removes only the repository's `truespec/.state/` directory.
Source Markdown and `truespec/config/project.json` remain untouched.

## Risks / Trade-offs

- [Users can lose local review history] -> Require an explicit reset flag and print the exact categories of data that will be deleted.
- [A stale service route can retain the old identity] -> Forget the root through the authenticated control API before deletion.
- [A malicious state symlink can redirect deletion] -> Revalidate containment and reject symlinked state.
- [Unsupported databases can block initialization] -> Preserve them unchanged and provide the exact recovery command.
