# Talkcer — State Machines

## Session
```text
Ready -> Active -> Paused -> Active
Active -> Completed
Any -> Archived
```

## Live capture
```text
Idle -> Starting -> Capturing
Capturing -> Paused
Capturing -> Stopping -> Idle
Paused -> Capturing
Paused -> Stopping -> Idle
Capturing -> Error
Error -> Recovering -> Idle
```

## STT stream
```text
Idle -> Connecting -> Streaming
Streaming -> Flushing -> Idle
Streaming -> Error
Error -> Reconnecting -> Streaming
Error -> Failed
```

## AI request
```text
Idle -> PreparingContext -> WaitingForModel -> Streaming -> Completed
WaitingForModel -> Cancelled
Streaming -> Cancelled
Any -> Failed
```

## Mock interview
```text
Ready -> AskingQuestion -> WaitingForAnswer -> Evaluating
Evaluating -> AskingQuestion
Evaluating -> Completed
WaitingForAnswer -> Completed
```

## Learning
```text
Ready -> Discussing -> WaitingForUser -> Thinking
Thinking -> Discussing
Discussing -> Completed
```

## Overlay
```text
Hidden -> Visible
Visible -> Hidden
Visible -> ClickThrough
ClickThrough -> Visible
Visible -> Closed
```
