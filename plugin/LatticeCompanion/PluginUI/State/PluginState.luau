--!strict

local PluginState = {}

function PluginState.new(ui: any)
	return {
		connection = ui:Value({
			status = "offline",
			label = "Lattice unavailable",
			latencyMs = nil,
			lastSuccessUnixMs = nil,
			errorCode = nil,
			errorMessage = nil,
		}),
		backend = ui:Value({
			provider = "Lattice Studio Bridge",
			environment = nil,
			place_name = nil,
			mcp = { state = "unavailable", detail = nil, latency_ms = nil },
			index_state = nil,
			recorder = { state = "unavailable" },
			connected_clients = {},
			capabilities = {},
			authorization = {},
			errors = {},
		}),
		studio = ui:Value({
			mode = "edit",
			placeName = game.Name,
			placeId = if game.PlaceId > 0 then game.PlaceId else nil,
		}),
		selection = ui:Value({
			count = 0,
			name = nil,
			className = nil,
			displayPath = nil,
			pathComponents = {},
			resourceRef = nil,
			resolution = "empty",
			references = nil,
			diagnostics = nil,
		}),
		activity = ui:Value({}),
		activitySamples = ui:Value(table.create(30, 0)),
		logs = ui:Value({}),
		recorder = ui:Value({ state = "unavailable" }),
		theme = ui:Value({}),
		layoutMode = ui:Value("normal"),
		view = ui:Value("main"),
		markerOpen = ui:Value(false),
		markerText = ui:Value(""),
		pairing = ui:Value({
			status = "discovering",
			label = "Discovering local Lattice",
			errorCode = nil,
			errorMessage = nil,
		}),
	}
end

return PluginState
