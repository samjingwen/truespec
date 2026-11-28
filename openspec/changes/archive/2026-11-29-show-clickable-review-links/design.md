# Design

## Context

See `proposal.md` for motivation.
The current plugin uses `session.synthetic({ resume: false })`, whose `description` is rendered as a plain timeline notice.
OpenCode V2 supports `session.prompt({ resume: false })` for a normal user message admitted without waking the agent loop.

## Goals / Non-Goals

**Goals:**

- Render compact Markdown links using OpenCode's normal message presentation.
- Preserve canonical URL validation and deterministic tailnet-then-local ordering.
- Prove command invocation creates no model request or assistant turn.
- Keep browser navigation under user control.

**Non-Goals:**

- Keep the link message out of future model context.
- Change connector protocol fields, review state, or browser routes.
- Add a custom OpenCode renderer or depend on synthetic notice Markdown support.

## Decisions

Build one Markdown string after existing URL validation.
Local mode uses `[Local](<local-url>)`.
Tailnet mode uses `[Tailnet](<selected-url>) · [Local](<local-url>)`.
The canonical URLs contain opaque IDs and no user-authored Markdown, so placing them in Markdown link destinations does not introduce label injection.

Admit the message with the normal prompt API and `resume: false`.
Do not provide a delivery mode that wakes the agent and do not call the synthetic API.
The isolated provider fixture will count model requests around command invocation and inspect normal user messages for exact Markdown while confirming no assistant response is created.

## Risks / Trade-offs

- [The message enters future model context] -> This is an explicit user-approved trade-off for normal clickable rendering.
- [OpenCode changes no-reply prompt semantics] -> The integration test fails if command invocation reaches the provider or creates an assistant turn.
- [A URL contains Markdown-significant characters] -> Existing URL construction uses opaque UUID path and query values, and plugin validation canonicalizes the complete URL first.

## Migration Plan

Refresh the generated OpenCode integration with `truespec init --integration opencode --force`.
Rollback restores the synthetic URL notice without changing service or review state.
