# Design

## Context

Submitted feedback currently exposes two independent concepts in the Review feed:

- Agent progress through a spinner or green check.
- Reviewer resolution through **Resolve / Reopen** actions and **Resolved / Unresolved** badges.

Reviewer resolution does not affect delivery, replies, review ending, source relocation, or whether the agent can act on feedback.
The database stores a `resolved` field, and older review history may contain true values.

The browser also disables spinner and panel animation under `prefers-reduced-motion: reduce`.
The current browser environment reports that preference, which makes the progress ring appear static.

## Goals / Non-Goals

**Goals:**

- Present one clear feedback lifecycle centered on agent progress.
- Preserve source-freshness warnings independently.
- Remove resolution controls and badges from all browser states.
- Keep existing databases and serialized review history readable.
- Use normal motion consistently regardless of the system preference.

**Non-Goals:**

- Remove the `resolved` SQLite column or rewrite historical rows.
- Change connector delivery, reply mirroring, or comment completion.
- Change the spinner shape, animation speed, drawer timing, or switch timing.
- Add a new motion preference inside TrueSpec.

## Decisions

### Remove resolution only from the product interface

Delete submitted-item **Resolve / Reopen** actions and **Resolved / Unresolved** badges.
Do not add a replacement action.

Keep the existing database column, Rust field, board payload, and mutation endpoint for backward and state compatibility.
The browser no longer reads or writes that field.
This avoids a schema migration and lets older binaries continue opening the same local database.

### Keep visible status ordered by importance

The visible sequence remains:

1. Working spinner or done check.
2. **Outdated** or source-missing warning when applicable.

Draft actions remain **Edit / Remove**.
Submitted comments have no action row unless another future action is introduced.

### Remove reduced-motion branching globally

Delete the `prefers-reduced-motion` media block.
Do not query or branch on the preference in JavaScript.

The normal drawer transitions, switch transitions, separator feedback, and spinner animation remain active for every user.
This is an explicit product decision and an acknowledged accessibility tradeoff.

### Update project guidance and contracts

Remove reduced-motion preservation from `AGENTS.md`.
Replace accepted resolution requirements with agent-progress requirements.
Remove resolution language from connector receipts and reply mirroring while retaining existing transport behavior.

## Risks / Trade-offs

- **Users lose a manual record of which comments they personally consider settled** -> Agent completion and source freshness become the only visible status by design.
- **Historical `resolved` values still exist but are invisible** -> Preserve them only for compatibility and document that the browser ignores them.
- **Motion-sensitive users cannot request reduced motion through the OS** -> Keep current motion values restrained, but deliberately apply them consistently.
- **Tests can pass while stale resolution labels remain in an edge state** -> Search the complete browser source and exercise queued, done, outdated, draft, and mirrored-reply states.

## Migration Plan

No database or state migration is required.
After rebuilding the embedded browser assets, existing reviews render without resolution controls or badges.
Historical resolution values remain unchanged in SQLite.
