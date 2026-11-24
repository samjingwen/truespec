# Proposal

## Why

Reviewer resolution duplicates the agent progress signal and adds controls and badges that are not needed to complete or end a review.
The static reduced-motion fallback also makes the working spinner appear broken, so TrueSpec will use consistent motion regardless of the operating-system preference.

## What Changes

- Remove **Resolve / Reopen** actions from submitted feedback.
- Remove **Resolved / Unresolved** status badges from the Review feed.
- Keep the existing SQLite resolution field and API data internally for compatibility with stored reviews, but stop exposing or mutating it in the browser.
- Keep visible review status focused on agent progress and source freshness: working spinner, green done check, and **Outdated** or source-missing warnings.
- Remove all `prefers-reduced-motion` overrides so drawers, switches, and spinners always use their normal motion.
- Update tests, accepted specifications, public guidance, and repository instructions for the simplified status and motion policy.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-annotations`: Remove reviewer resolution from the visible chronological feed and submitted-item actions.
- `review-handoff`: Present agent progress without a separate reviewer-controlled resolution state.
- `connector-protocol`: Stop defining delivery and mirrored replies in terms of reviewer resolution.
- `document-reading`: Use the same interface motion regardless of the operating-system reduced-motion preference.

## Impact

- Affects browser status rendering, submitted-item actions, CSS motion policy, browser tests, OpenSpec contracts, README, and `AGENTS.md`.
- Does not migrate or remove the existing `resolved` database column, alter connector delivery, or change source freshness and relocation behavior.
