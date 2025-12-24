# ADR-0005: Session State Machine

## Status

Accepted

## Context

Recording sessions have multiple states and transitions that need to be managed:

- Idle (ready to start)
- Preparing (loading model, setting up audio)
- Recording (actively capturing and transcribing)
- Stopping (finishing pending transcriptions)
- Completed (session done, transcript available)
- Error (something went wrong)

We need a clear model to:
- Prevent invalid state transitions
- Communicate state to the frontend
- Handle errors gracefully
- Support session reset

## Decision

We implemented an **explicit state machine** in the `SessionManager`.

State transitions:
```
Idle -> Preparing -> Recording -> Stopping -> Completed
  ^                      |            |           |
  |                      v            v           |
  +------- Error <-------+------------+-----------+
  |                                               |
  +<------------------- Reset <-------------------+
```

Key design choices:
- State is an enum with explicit variants
- Transitions are validated (return `Result`)
- Error state captures the error message
- Reset is always allowed from any state

## Consequences

### Positive

- Clear, predictable state transitions
- Easy to communicate state to frontend via IPC
- Error handling is explicit
- Testable state machine logic

### Negative

- More code than ad-hoc state management
- Some transitions require careful ordering
- State changes need to be emitted to frontend

## References

- [State Machine Pattern](https://refactoring.guru/design-patterns/state)
- [Typestate Pattern in Rust](https://cliffle.com/blog/rust-typestate/)
