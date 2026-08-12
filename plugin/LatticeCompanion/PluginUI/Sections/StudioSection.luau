--!strict

local Primitives = require(script.Parent.Parent.Components.Primitives)

local StudioSection = {}

function StudioSection.create(ui: any, state: any, position: any, size: any): Frame
	return Primitives.section(ui, state, "Studio", position, size, {
		Primitives.metric(
			ui,
			state,
			"Mode",
			ui:Computed(function(use)
				local mode = use(state.studio).mode
				return string.upper(string.sub(mode, 1, 1)) .. string.sub(mode, 2)
			end),
			37
		),
		Primitives.metric(
			ui,
			state,
			"Selection",
			ui:Computed(function(use)
				local selected = use(state.selection)
				if selected.count == 0 then
					return "None"
				elseif selected.count == 1 then
					return selected.name
				end
				return tostring(selected.count) .. " Instances"
			end),
			60
		),
		Primitives.metric(
			ui,
			state,
			"Index",
			ui:Computed(function(use)
				return use(state.backend).index_state or "Unavailable"
			end),
			83
		),
	})
end

return StudioSection
