# Design

## Context

See `proposal.md` for motivation.
Connector protocol v3 already returns a validated selected URL and local loopback URL, and the OpenCode integration currently emits one synthetic message prefixed with `TrueSpec review:`.

## Goals / Non-Goals

**Goals:**

- Derive presentation from the already validated canonical URL objects.
- Emit only one or two URL lines in deterministic selected-then-local order.
- Remove automatic browser launching while keeping stored connector bindings unchanged.
- Verify exact synthetic-message text through the isolated OpenCode flow.

**Non-Goals:**

- Change attachment fields or advance the connector protocol.
- Add labels, Markdown links, status prose, or browser UI.
- Persist either URL in connector binding storage.
- Launch or select a browser on the user's behalf.

## Decisions

Build the synthetic text from the selected canonical URL followed by the local canonical URL only when they differ.
Comparing canonical `href` values avoids duplicating the loopback URL in local mode without coupling presentation to tailnet metadata.
Remove the operating-system browser helper and do not pass either URL to a launcher.
The user chooses which emitted URL to open.

The isolated OpenCode test will inspect synthetic message text rather than relying only on URL extraction.
Generated-plugin coverage will verify that no browser-opening helper or invocation remains.
README guidance will show the exact line-only output contract.

## Risks / Trade-offs

- [A label-free message gives no inline explanation] -> The command description and README provide context, while the requested output remains directly copyable.
- [A future attachment returns equivalent URLs with different spelling] -> Existing canonical URL validation normalizes both values before presentation.
- [Removing automatic launch adds one user action] -> Both canonical choices remain immediately copyable in the coding conversation.

## Migration Plan

Refresh the generated OpenCode integration with `truespec init --integration opencode --force`.
Rollback restores the previous prefixed single-URL text and automatic browser launch without changing service or review state.
