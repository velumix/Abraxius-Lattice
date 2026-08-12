--!strict

-- Development-only visual fixtures. Production never imports this module.
return table.freeze({
	Disconnected = { connection = { status = "offline", label = "Lattice unavailable" } },
	Connecting = { connection = { status = "connecting", label = "Connecting" } },
	Connected = { connection = { status = "connected", label = "Connected", latencyMs = 18 } },
	Degraded = { connection = { status = "degraded", label = "Degraded", latencyMs = 1840 } },
	Error = { connection = { status = "error", label = "Authentication failed" } },
	Recording = { recorder = { state = "recording", mode = "diagnostic", events = 1_300_000 } },
	SelectionResolved = {
		selection = {
			count = 1,
			name = "VehicleService",
			className = "ModuleScript",
			resourceRef = "rbx://workspace/test/script/test",
		},
	},
	Narrow = { layoutMode = "narrow" },
	Wide = { layoutMode = "wide" },
})
