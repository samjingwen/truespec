# Tasks

## 1. Present clickable review links

- [x] 1.1 Replace the synthetic notice with a normal `resume: false` Markdown message containing exact local or tailnet-and-local links, and verify isolated OpenCode coverage proves no model request, assistant turn, or browser launch occurs
- [x] 1.2 Document compact clickable link output and the future-context trade-off in README, and verify generated-plugin tests cover the normal no-reply prompt fields and absence of synthetic presentation

## 2. Integration verification

- [x] 2.1 Run formatting, Clippy, Rust tests, strict OpenSpec validation, isolated OpenCode, public-repository audit, and exact-head Hardlook; address findings and rerun every invalidated check

## Workflow follow-up

- Sync the connector protocol delta and archive the change after implementation review.
- Require hosted `Quality` and `Browser and OpenCode` checks before merge.
