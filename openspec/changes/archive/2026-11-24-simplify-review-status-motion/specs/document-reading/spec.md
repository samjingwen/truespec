# Document Reading Delta

## ADDED Requirements

### Requirement: Use consistent interface motion

TrueSpec SHALL use its normal interface transitions and progress animations regardless of the operating system's reduced-motion preference.

#### Scenario: Open TrueSpec with reduced motion enabled

- **WHEN** the browser reports `prefers-reduced-motion: reduce`
- **THEN** drawer transitions, switch transitions, resize feedback, and progress spinners use the same motion as the default preference

#### Scenario: Show agent working with reduced motion enabled

- **WHEN** submitted feedback is waiting for the agent and the browser reports reduced motion
- **THEN** the working spinner continues rotating
