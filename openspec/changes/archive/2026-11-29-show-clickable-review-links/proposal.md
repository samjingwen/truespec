# Proposal

## Why

OpenCode renders synthetic review output as a muted one-line notice, so raw review URLs are visually noisy and compact labels cannot be clickable.
A normal no-reply Markdown message can present clear links without launching a browser or starting an agent turn.

## What Changes

- Replace the synthetic review notice with a normal Markdown user message admitted with `resume: false`.
- In local mode, display one clickable **Local** link.
- In tailnet mode, display clickable **Tailnet** and **Local** links in that order.
- Keep automatic browser launching disabled.
- Accept that the no-reply link message becomes part of future conversation context.
- Verify that invoking `/truespec-review` does not issue a model request or create an assistant turn.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connector-protocol`: Present compact clickable review links through a normal no-reply conversation message.

## Impact

- Affects the generated OpenCode plugin, isolated OpenCode tests, README guidance, and the accepted connector protocol.
- Does not change attachment fields, protocol version, review routing, connector bindings, service lifecycle, or browser security.
