--!strict

local Primitives = require(script.Parent.Parent.Components.Primitives)

local ConnectionSection = {}

local function humanize(value: string?): string
	if value == nil then
		return "Unavailable"
	end
	local spaced = string.gsub(value, "_", " ")
	return string.upper(string.sub(spaced, 1, 1)) .. string.sub(spaced, 2)
end

function ConnectionSection.create(ui: any, state: any, position: any, size: any, onRetry: () -> ()): Frame
	return Primitives.section(ui, state, "Connection", position, size, {
		Primitives.label(ui, state, {
			Name = "Status",
			Text = ui:Computed(function(use)
				local connection = use(state.connection)
				local icon = if connection.status == "connected"
					then "● "
					elseif connection.status == "error" then "✕ "
					else "○ "
				local pairing = use(state.pairing)
				local label = if connection.status == "offline" and pairing.status ~= "paired"
					then pairing.label
					else connection.label
				return icon .. label
			end),
			ColorToken = "TextPrimary",
			Font = Enum.Font.BuilderSansMedium,
			Position = UDim2.fromOffset(12, 36),
			Size = UDim2.new(1, -100, 0, 24),
		}),
		Primitives.metric(
			ui,
			state,
			"Session",
			ui:Computed(function(use)
				local backend = use(state.backend)
				local backendPlace = backend.place_name
				if type(backendPlace) == "string" and backendPlace ~= "" then
					return backendPlace
				end
				if type(backend.mcp) == "table" and backend.mcp.state == "connected" then
					return "Resolving place..."
				end
				return use(state.studio).placeName or "Unknown"
			end),
			64,
			nil
		),
		Primitives.metric(
			ui,
			state,
			"Environment",
			ui:Computed(function(use)
				local environment = use(state.backend).environment
				if environment == nil then
					return "Unavailable"
				end
				return humanize(environment.host_platform) .. " · " .. humanize(environment.runtime)
			end),
			87,
			nil
		),
		Primitives.metric(
			ui,
			state,
			"Bridge",
			ui:Computed(function(use)
				local connection = use(state.connection)
				local latency = connection.latencyMs
				return humanize(connection.status)
					.. if latency ~= nil then " · " .. tostring(latency) .. " ms" else ""
			end),
			110,
			nil
		),
		Primitives.button(ui, state, {
			Name = "Retry",
			Text = "Retry",
			Position = UDim2.new(1, -76, 1, -33),
			Size = UDim2.fromOffset(64, 24),
			Visible = ui:Computed(function(use)
				local status = use(state.connection).status
				return status ~= "connected"
			end),
			OnActivated = onRetry,
		}),
	})
end

return ConnectionSection
