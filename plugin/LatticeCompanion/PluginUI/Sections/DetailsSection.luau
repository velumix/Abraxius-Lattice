--!strict

local Primitives = require(script.Parent.Parent.Components.Primitives)

local DetailsSection = {}

local function optional(value: any): string
	return if value == nil then "Unavailable" else tostring(value)
end

function DetailsSection.create(ui: any, state: any, position: any, size: any, onBack: () -> ()): Frame
	return Primitives.section(ui, state, "Connection Details", position, size, {
		Primitives.metric(
			ui,
			state,
			"Provider",
			ui:Computed(function(use)
				return use(state.backend).provider or "Unavailable"
			end),
			38
		),
		Primitives.metric(
			ui,
			state,
			"Protocol",
			ui:Computed(function(use)
				local mcp = use(state.backend).mcp
				if mcp == nil or mcp.state == nil or mcp.state == "unavailable" then
					return "Unavailable"
				end
				local label = string.upper(string.sub(mcp.state, 1, 1)) .. string.sub(mcp.state, 2)
				if mcp.latency_ms ~= nil then
					return label .. " · " .. tostring(mcp.latency_ms) .. " ms"
				end
				return label
			end),
			62
		),
		Primitives.metric(
			ui,
			state,
			"Environment ID",
			ui:Computed(function(use)
				local environment = use(state.backend).environment
				return if environment == nil then "Unavailable" else environment.environment_id
			end),
			86
		),
		Primitives.metric(
			ui,
			state,
			"Studio PID",
			ui:Computed(function(use)
				local environment = use(state.backend).environment
				return if environment == nil then "Unavailable" else optional(environment.process_id)
			end),
			110
		),
		Primitives.metric(
			ui,
			state,
			"Deployment",
			ui:Computed(function(use)
				local environment = use(state.backend).environment
				return if environment == nil then "Unavailable" else optional(environment.deployment)
			end),
			134
		),
		Primitives.metric(
			ui,
			state,
			"Capabilities",
			ui:Computed(function(use)
				return tostring(#(use(state.backend).capabilities or {}))
			end),
			158
		),
		Primitives.metric(
			ui,
			state,
			"Clients",
			ui:Computed(function(use)
				return tostring(#(use(state.backend).connected_clients or {}))
			end),
			182
		),
		Primitives.button(ui, state, {
			Text = "Back",
			Position = UDim2.fromOffset(12, 218),
			Size = UDim2.fromOffset(70, 28),
			OnActivated = onBack,
		}),
	})
end

return DetailsSection
