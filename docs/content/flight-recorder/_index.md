+++
title = "Flight Recorder"
weight = 8
+++

Flight Recorder is the runtime-history layer for Lattice. It records bounded,
versioned evidence about Studio, processes, MCP operations, profiler data,
DataModel changes, and anomalies. It is not a generic log window and it does
not invent causality.

Tool-fabric operations can emit `ToolCallStarted`, `ToolCallCompleted`, and
`ToolCallFailed` events with the same `OperationId` used by audit and result
storage. Correlation presents evidence and confidence; it does not replace
engineering judgment.

If a runtime capability is unavailable, the UI and documentation must report
it as unavailable rather than showing synthetic recording state.
