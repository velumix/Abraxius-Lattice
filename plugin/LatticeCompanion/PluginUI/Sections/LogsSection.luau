--!strict

local Primitives = require(script.Parent.Parent.Components.Primitives)

local LogsSection = {}

local function formatTime(unixMs: number): string
	local seconds = math.floor(unixMs / 1000)
	local date = DateTime.fromUnixTimestamp(seconds):ToLocalTime()
	return string.format("%02d:%02d:%02d", date.Hour, date.Minute, date.Second)
end

local function logRow(ui: any, state: any, index: number): TextLabel
	return Primitives.label(ui, state, {
		Name = "Log" .. tostring(index),
		Text = ui:Computed(function(use)
			local entry = use(state.logs)[index]
			return if entry == nil then "" else formatTime(entry.unixMs) .. "  " .. entry.message
		end),
		ColorToken = "TextSecondary",
		Font = Enum.Font.RobotoMono,
		TextSize = 11,
		Position = UDim2.fromOffset(12, 35 + ((index - 1) * 22)),
		Size = UDim2.new(1, -24, 0, 21),
		Visible = ui:Computed(function(use)
			return use(state.logs)[index] ~= nil
		end),
	})
end

function LogsSection.create(ui: any, state: any, position: any, size: any, onBack: () -> ()): Frame
	local rows = {}
	for index = 1, 12 do
		table.insert(rows, logRow(ui, state, index))
	end
	table.insert(
		rows,
		Primitives.label(ui, state, {
			Text = ui:Computed(function(use)
				return if #use(state.logs) == 0 then "No recent plugin events" else ""
			end),
			ColorToken = "TextMuted",
			Position = UDim2.fromOffset(12, 37),
			Size = UDim2.new(1, -24, 0, 22),
		})
	)
	table.insert(
		rows,
		Primitives.button(ui, state, {
			Text = "Back",
			Position = UDim2.new(0, 12, 1, -40),
			Size = UDim2.fromOffset(70, 28),
			OnActivated = onBack,
		})
	)
	return Primitives.section(ui, state, "Recent Logs", position, size, rows)
end

return LogsSection
