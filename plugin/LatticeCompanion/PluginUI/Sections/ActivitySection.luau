--!strict

local Primitives = require(script.Parent.Parent.Components.Primitives)
local GraphModule = require(script.Parent.Parent.Components.GraphModule)

local ActivitySection = {}

local function iconFor(semantics: string, status: string): string
	if status == "failed" then
		return "!"
	elseif status == "executing" then
		return "◉"
	elseif semantics == "destructive" then
		return "▲"
	elseif semantics == "mutation" then
		return "◆"
	end
	return "↳"
end

local function activityRow(ui: any, state: any, index: number): Frame
	return ui:New("Frame", {
		Name = "ActivityRow" .. tostring(index),
		BackgroundTransparency = 1,
		Position = UDim2.fromOffset(12, 39 + ((index - 1) * 24)),
		Size = UDim2.new(1, -24, 0, 22),
		Visible = ui:Computed(function(use)
			return use(state.activity)[index] ~= nil
		end),
		[ui:childrenKey()] = {
			Primitives.label(ui, state, {
				Text = ui:Computed(function(use)
					local row = use(state.activity)[index]
					return if row == nil then "" else iconFor(row.semantics, row.status)
				end),
				ColorToken = "Accent",
				TextXAlignment = Enum.TextXAlignment.Center,
				Size = UDim2.fromOffset(18, 22),
			}),
			Primitives.label(ui, state, {
				Text = ui:Computed(function(use)
					local row = use(state.activity)[index]
					return if row == nil then "" else row.name
				end),
				Font = Enum.Font.RobotoMono,
				TextSize = 11,
				Position = UDim2.fromOffset(23, 0),
				Size = UDim2.new(1, -88, 1, 0),
			}),
			Primitives.label(ui, state, {
				Text = ui:Computed(function(use)
					local row = use(state.activity)[index]
					if row == nil then
						return ""
					end
					return if row.durationMs ~= nil then tostring(row.durationMs) .. " ms" else row.status
				end),
				ColorToken = "TextSecondary",
				TextXAlignment = Enum.TextXAlignment.Right,
				Position = UDim2.new(1, -65, 0, 0),
				Size = UDim2.fromOffset(65, 22),
			}),
		},
	}) :: Frame
end

function ActivitySection.create(ui: any, state: any, position: any, size: any): Frame
	return Primitives.section(ui, state, "Activity", position, size, {
		activityRow(ui, state, 1),
		activityRow(ui, state, 2),
		activityRow(ui, state, 3),
		Primitives.label(ui, state, {
			Text = ui:Computed(function(use)
				return if #use(state.activity) == 0 then "No recent operations" else "MCP activity"
			end),
			ColorToken = "TextMuted",
			TextSize = 11,
			Position = UDim2.fromOffset(12, 114),
			Size = UDim2.fromOffset(90, 20),
		}),
		GraphModule.create(ui, state, UDim2.fromOffset(103, 114), UDim2.new(1, -115, 0, 28)),
	})
end

return ActivitySection
