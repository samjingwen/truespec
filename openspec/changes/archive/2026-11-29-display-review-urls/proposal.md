# Proposal

## Why

The OpenCode `/truespec-review` command currently prints only the selected tailnet URL, which hides the still-valid loopback URL from the host user.
Showing the available review URLs as plain URL lines makes both access paths immediately copyable without adding explanatory text.

## What Changes

- Make the OpenCode command output contain only review URL lines.
- In local mode, output the local loopback review URL once.
- In tailnet mode, output the selected tailnet review URL first and the local loopback review URL second.
- Stop launching a browser automatically in local and tailnet modes.
- Document the command output and verify exact synthetic-message content in the isolated OpenCode flow.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `connector-protocol`: Define how an installed integration presents selected and local review URLs to the paired coding conversation.

## Impact

- Affects the generated OpenCode plugin, connector presentation tests, README guidance, and the accepted connector protocol.
- Does not change attachment payloads, protocol version, browser routing, review identity, or stored connector bindings.
