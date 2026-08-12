# Provider ABI

`ToolProvider` is the internal protocol-neutral provider contract. Implementations supply a stable descriptor and asynchronous `connect`, `disconnect`, `list_tools`, and `call` operations. Futures are boxed to keep the trait object-safe without exposing a protocol SDK.

Registration is lazy: `ConnectionBroker::register` records a provider but performs no I/O. `connect` creates a `ProviderConnection`, refreshes the catalog, and verifies that the returned provider identity matches the registered provider. Per-provider and global semaphores bound calls.

Imported tools must include their native name and schemas. Semantic capability bindings and verified operation semantics are explicit adapter data; they must never be inferred from descriptions. Raw tools use an empty capability list and unknown verified semantics.

External stdio implementations must use an absolute executable plus argument vector. Shell commands, implicit expansion, inherited daemon environments, and plaintext credentials are forbidden. Provider credentials will use secret references when the secret-store phase is implemented.

MCP is the initial external provider ABI because it supplies process isolation, initialization, schema/tool discovery, structured calls, resources, and lifecycle errors. Arbitrary dynamic native-library loading is not supported.
