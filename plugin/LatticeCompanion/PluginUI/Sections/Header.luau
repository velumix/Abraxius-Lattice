--!strict

local Primitives = require(script.Parent.Parent.Components.Primitives)
local Typography = require(script.Parent.Parent.Theme.Typography)

local Header = {}

local LATTICE_ICON = "rbxassetid://115283647281192"

local function statusPresentation(status: string): (string, string, string)
	if status == "connected" then
		return "●", "LIVE", "Success"
	elseif status == "connecting" or status == "busy" then
		return "◉", "BUSY", "Accent"
	elseif status == "degraded" then
		return "⚠", "DEGRADED", "Warning"
	elseif status == "error" then
		return "✕", "ERROR", "Danger"
	elseif status == "recording" then
		return "●", "REC", "Recording"
	end
	return "○", "OFF", "TextMuted"
end

function Header.create(ui: any, state: any, onDetails: () -> ()): Frame
	return ui:New("Frame", {
		Name = "Header",
		BackgroundColor3 = ui:Computed(function(use)
			return use(state.theme).Background
		end),
		BorderSizePixel = 0,
		Size = UDim2.new(1, 0, 0, 58),
		[ui:childrenKey()] = {
			ui:New("ImageLabel", {
				Name = "Mark",
				BackgroundTransparency = 1,
				Image = LATTICE_ICON,
				ScaleType = Enum.ScaleType.Fit,
				Position = UDim2.fromOffset(10, 9),
				Size = UDim2.fromOffset(24, 24),
			}),
			Primitives.label(ui, state, {
				Name = "Product",
				Text = "LATTICE",
				TextSize = Typography.Header,
				Font = Typography.FontMedium,
				Position = UDim2.fromOffset(35, 7),
				Size = UDim2.new(1, -150, 0, 25),
			}),
			Primitives.label(ui, state, {
				Name = "Subtitle",
				Text = "Studio Bridge",
				ColorToken = "TextSecondary",
				TextSize = Typography.Metadata,
				Position = UDim2.fromOffset(36, 31),
				Size = UDim2.new(1, -155, 0, 17),
			}),
			Primitives.statusChip(ui, state, {
				Icon = ui:Computed(function(use)
					local icon = statusPresentation(use(state.connection).status)
					return icon
				end),
				Text = ui:Computed(function(use)
					local _, label = statusPresentation(use(state.connection).status)
					local clients = #(use(state.backend).connected_clients or {})
					return if clients > 0 then label .. " · " .. tostring(clients) else label
				end),
				IconColor3 = ui:Computed(function(use)
					local _, _, colorToken = statusPresentation(use(state.connection).status)
					return use(state.theme)[colorToken]
				end),
				Position = UDim2.new(1, -100, 0, 10),
				Size = UDim2.fromOffset(88, 24),
			}),
			Primitives.button(ui, state, {
				Name = "Details",
				Text = "···",
				Position = UDim2.new(1, -42, 0, 35),
				Size = UDim2.fromOffset(30, 20),
				OnActivated = onDetails,
			}),
		},
	}) :: Frame
end

return Header
