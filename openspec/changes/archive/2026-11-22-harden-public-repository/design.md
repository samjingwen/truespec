# Design

## Context

TrueSpec embeds a base SQLite schema and append-only migration files, then advances databases according to `PRAGMA user_version` inside one immediate transaction.
Version 3 originally required `agent_replies.connector_id`, while version 4 rebuilt that table with a nullable connector ID for manual replies.
The version 3 file was later edited to contain the version 4 nullability, which violates the repository's migration invariant even though every supported database currently finishes at version 4.

The public manual command is now `truespec listen`, but the browser disclosure, one CLI completion message, README text, and accepted scenario still use polling terminology.
Connector-internal delivery intentionally retains its private poll operation.

## Goals / Non-Goals

**Goals:**

- Restore the exact historical version 3 schema and prove that a populated version 3 database upgrades safely.
- Use listening terminology wherever a reviewer or manual agent sees the long-running workflow.
- Align the public OpenCode walkthrough with question-only review feedback.
- Remove repository placeholders that no longer preserve empty directories.

**Non-Goals:**

- Replace `rusqlite`, `PRAGMA user_version`, or the current migration runner.
- Change the version 4 schema or add a version 5 migration.
- Rename the connector protocol's internal `poll` operation.
- Add continuous integration or alter release and package publication policy.

## Decisions

### Restore version 3 instead of compensating with another migration

Change `schema-v3.sql` back to its published `connector_id TEXT NOT NULL` definition.
Keep `schema-v4.sql` as the only migration that introduces nullable connector IDs.
This restores a canonical replay path without changing the final version 4 database.

Adding another migration would not repair the rewritten historical artifact and would make the migration sequence less clear.

### Test migration replay from an actual version 3 database

Build a temporary database by applying the base schema and migrations through version 3, seed a connector-backed reply, then open it through the current store.
Verify that the database reaches version 4, preserves the reply and feed entry, and makes `agent_replies.connector_id` nullable.
This checks both historical replay and the final shape instead of only testing fresh database creation.

### Separate manual language from connector protocol language

Use **Agent not listening?** in the browser and "stop listening" in manual CLI guidance.
Keep `connector poll`, `Outcome::Idle`, and protocol documentation unchanged because they describe the private connector transport rather than the manual command.

### Keep question-only behavior explicit in public guidance

Change the OpenCode walkthrough from always updating Markdown to answering questions or applying clearly requested Markdown changes.
The implementation and accepted `Support questions and change requests` requirement already provide this behavior, so no new runtime logic is needed.

## Risks / Trade-offs

- **Historical migration restoration could expose an invalid replay assumption** -> Seed and upgrade a version 3 database before accepting the change.
- **Broad replacement of polling language could rename connector protocol concepts accidentally** -> Limit replacements to the manual disclosure, manual status guidance, README, and matching review-handoff scenario.
- **Removing placeholders could make empty directories disappear later** -> The directories now contain accepted specifications and archived changes, so placeholders no longer serve a purpose.

## Migration Plan

No user data migration or rollback action is required.
Version 4 remains the current schema, and existing version 4 databases are unchanged.
Apply the repository cleanup, run the full Rust and OpenSpec gates, rebuild embedded browser assets, and verify desktop and phone review flows.
