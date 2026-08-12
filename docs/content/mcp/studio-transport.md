+++
title = "Studio MCP Transport"
weight = 2
+++

Studio MCP is a stdio provider launched through the resolved Studio
environment. On Linux/Vinegar, launch support is experimental and must use the
resolved Wine runtime and deployment information from `lattice-platform`.

A valid connection requires all of the following:

1. A real MCP child is launched without a shell command string.
2. MCP negotiation succeeds for the provider's protocol profile.
3. A read-only Studio call returns a real result.
4. The result is correlated with the correct `StudioSession` and environment.
5. The child is disconnected and reaped without terminating Studio.

Do not treat binary presence, a guessed port, or a fabricated session as
connectivity.
