--!strict

local Typography = require(script.Parent.Parent.Theme.Typography)

local Primitives = {}

local function token(ui: any, state: any, name: string): any
	return ui:Computed(function(use)
		return use(state.theme)[name]
	end)
end

function Primitives.label(ui: any, state: any, props: { [string]: any }): TextLabel
	return ui:New("TextLabel", {
		Name = props.Name or "Label",
		BackgroundTransparency = 1,
		BorderSizePixel = 0,
		Font = props.Font or Typography.Font,
		Text = props.Text or "",
		TextColor3 = props.TextColor3 or token(ui, state, props.ColorToken or "TextPrimary"),
		TextSize = props.TextSize or Typography.Body,
		TextXAlignment = props.TextXAlignment or Enum.TextXAlignment.Left,
		TextYAlignment = props.TextYAlignment or Enum.TextYAlignment.Center,
		TextTruncate = props.TextTruncate or Enum.TextTruncate.AtEnd,
		TextWrapped = props.TextWrapped or false,
		Position = props.Position or UDim2.fromOffset(0, 0),
		Size = props.Size or UDim2.fromScale(1, 1),
		Visible = if props.Visible == nil then true else props.Visible,
		ZIndex = props.ZIndex or 1,
	}) :: TextLabel
end

function Primitives.divider(ui: any, state: any, y: number): Frame
	return ui:New("Frame", {
		Name = "Divider",
		BackgroundColor3 = token(ui, state, "Border"),
		BorderSizePixel = 0,
		Position = UDim2.fromOffset(0, y),
		Size = UDim2.new(1, 0, 0, 1),
	}) :: Frame
end

function Primitives.section(ui: any, state: any, title: string, position: any, size: any, children: { any }): Frame
	local allChildren = {
		Primitives.label(ui, state, {
			Name = "SectionTitle",
			Text = string.upper(title),
			ColorToken = "TextMuted",
			TextSize = Typography.Metadata,
			Font = Typography.FontMedium,
			Position = UDim2.fromOffset(12, 7),
			Size = UDim2.new(1, -24, 0, 18),
		}),
		Primitives.divider(ui, state, 29),
	}
	for _, child in children do
		table.insert(allChildren, child)
	end
	return ui:New("Frame", {
		Name = title .. "Section",
		BackgroundColor3 = token(ui, state, "Surface"),
		BorderSizePixel = 0,
		Position = position,
		Size = size,
		ClipsDescendants = true,
		[ui:childrenKey()] = {
			ui:New("UICorner", { CornerRadius = UDim.new(0, 4) }),
			ui:New("UIStroke", {
				Color = token(ui, state, "Border"),
				Thickness = 1,
				Transparency = 0.55,
			}),
			allChildren,
		},
	}) :: Frame
end

function Primitives.button(ui: any, state: any, props: { [string]: any }): TextButton
	local enabled = if props.Enabled == nil then true else props.Enabled
	return ui:New("TextButton", {
		Name = props.Name or "Button",
		AutoButtonColor = false,
		Active = enabled,
		BackgroundColor3 = ui:Computed(function(use)
			if not use(enabled) then
				return use(state.theme).SurfaceRaised
			end
			return use(state.theme)[props.BackgroundToken or "SurfaceHover"]
		end),
		BorderSizePixel = 0,
		Font = Typography.FontMedium,
		Text = props.Text,
		TextColor3 = ui:Computed(function(use)
			return if use(enabled) then use(state.theme).TextPrimary else use(state.theme).TextMuted
		end),
		TextSize = Typography.Secondary,
		Position = props.Position,
		Size = props.Size,
		Visible = if props.Visible == nil then true else props.Visible,
		[ui:onEventKey("Activated")] = function()
			if ui:peek(enabled) and props.OnActivated ~= nil then
				props.OnActivated()
			end
		end,
		[ui:childrenKey()] = {
			ui:New("UICorner", { CornerRadius = UDim.new(0, 5) }),
			ui:New("UIStroke", {
				Color = token(ui, state, "BorderStrong"),
				Thickness = 1,
				Transparency = 0.25,
			}),
		},
	}) :: TextButton
end

function Primitives.metric(ui: any, state: any, labelText: string, value: any, y: number, visible: any?): Frame
	return ui:New("Frame", {
		Name = labelText .. "Metric",
		BackgroundTransparency = 1,
		Position = UDim2.fromOffset(12, y),
		Size = UDim2.new(1, -24, 0, 22),
		Visible = if visible == nil then true else visible,
		[ui:childrenKey()] = {
			Primitives.label(ui, state, {
				Text = labelText,
				ColorToken = "TextSecondary",
				TextSize = Typography.Secondary,
				Size = UDim2.fromScale(0.42, 1),
			}),
			Primitives.label(ui, state, {
				Text = value,
				TextXAlignment = Enum.TextXAlignment.Right,
				TextSize = Typography.Secondary,
				Position = UDim2.new(0.42, 4, 0, 0),
				Size = UDim2.new(0.58, -4, 1, 0),
			}),
		},
	}) :: Frame
end

function Primitives.statusChip(ui: any, state: any, props: { [string]: any }): Frame
	return ui:New("Frame", {
		Name = "StatusChip",
		BackgroundColor3 = token(ui, state, "SurfaceRaised"),
		BorderSizePixel = 0,
		Position = props.Position,
		Size = props.Size,
		[ui:childrenKey()] = {
			ui:New("UICorner", { CornerRadius = UDim.new(1, 0) }),
			Primitives.label(ui, state, {
				Text = props.Icon,
				ColorToken = props.ColorToken,
				TextColor3 = props.IconColor3,
				TextSize = Typography.Secondary,
				TextXAlignment = Enum.TextXAlignment.Center,
				Position = UDim2.fromOffset(7, 0),
				Size = UDim2.fromOffset(14, 24),
			}),
			Primitives.label(ui, state, {
				Text = props.Text,
				TextSize = Typography.Metadata,
				Font = Typography.FontMedium,
				Position = UDim2.fromOffset(22, 0),
				Size = UDim2.new(1, -29, 1, 0),
			}),
		},
	}) :: Frame
end

return Primitives
