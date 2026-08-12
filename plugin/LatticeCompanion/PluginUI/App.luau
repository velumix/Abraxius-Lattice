--!strict

local Header = require(script.Parent.Sections.Header)
local ConnectionSection = require(script.Parent.Sections.ConnectionSection)
local ActivitySection = require(script.Parent.Sections.ActivitySection)
local StudioSection = require(script.Parent.Sections.StudioSection)
local SelectionSection = require(script.Parent.Sections.SelectionSection)
local RecorderSection = require(script.Parent.Sections.RecorderSection)
local DetailsSection = require(script.Parent.Sections.DetailsSection)
local LogsSection = require(script.Parent.Sections.LogsSection)
local Primitives = require(script.Parent.Components.Primitives)

local App = {}

local LATTICE_ICON = "rbxassetid://115283647281192"

local function sectionPosition(ui: any, state: any, section: string): any
	return ui:Computed(function(use)
		local mode = use(state.layoutMode)
		if mode == "wide" then
			local positions = {
				connection = UDim2.fromOffset(8, 8),
				studio = UDim2.new(0.5, 4, 0, 8),
				activity = UDim2.fromOffset(8, 160),
				selection = UDim2.new(0.5, 4, 0, 126),
				recorder = UDim2.fromOffset(8, 318),
			}
			return positions[section]
		end
		local positions = {
			connection = UDim2.fromOffset(8, 8),
			activity = UDim2.fromOffset(8, 160),
			studio = UDim2.fromOffset(8, 312),
			selection = UDim2.fromOffset(8, 430),
			recorder = UDim2.fromOffset(8, 578),
		}
		return positions[section]
	end)
end

local function sectionSize(ui: any, state: any, section: string): any
	return ui:Computed(function(use)
		local wide = use(state.layoutMode) == "wide"
		local heights = {
			connection = 144,
			activity = 144,
			studio = 110,
			selection = 140,
			recorder = 106,
		}
		return UDim2.new(
			if wide and section ~= "recorder" then 0.5 else 1,
			if wide and section ~= "recorder" then -12 else -16,
			0,
			heights[section]
		)
	end)
end

function App.mount(plugin: Plugin, ui: any, state: any, controllers: any)
	local widgetInfo = DockWidgetPluginGuiInfo.new(Enum.InitialDockState.Right, false, false, 340, 520, 280, 320)
	local widget = plugin:CreateDockWidgetPluginGui("AbraxiusLatticeCockpit", widgetInfo)
	widget.Title = "Lattice"
	widget.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
	ui:addCleanup(widget)

	local toolbar = plugin:CreateToolbar("Lattice")
	local toolbarButton =
		toolbar:CreateButton("Lattice", "Lattice Studio Bridge — toggle cockpit", LATTICE_ICON, "Lattice")
	toolbarButton.ClickableWhenViewportHidden = true
	ui:addCleanup(toolbarButton.Click:Connect(function()
		widget.Enabled = not widget.Enabled
		toolbarButton:SetActive(widget.Enabled)
	end))
	ui:addCleanup(widget:GetPropertyChangedSignal("Enabled"):Connect(function()
		toolbarButton:SetActive(widget.Enabled)
	end))

	local function updateLayout()
		local width = widget.AbsoluteSize.X
		state.layoutMode:set(if width < 320 then "narrow" elseif width > 520 then "wide" else "normal")
	end
	ui:addCleanup(widget:GetPropertyChangedSignal("AbsoluteSize"):Connect(updateLayout))
	updateLayout()

	local mainVisible = ui:Computed(function(use)
		return use(state.view) == "main"
	end)
	local detailsVisible = ui:Computed(function(use)
		return use(state.view) == "details"
	end)
	local logsVisible = ui:Computed(function(use)
		return use(state.view) == "logs"
	end)
	local markerVisible = ui:Computed(function(use)
		return use(state.markerOpen)
	end)

	local content = ui:New("ScrollingFrame", {
		Name = "Content",
		BackgroundTransparency = 1,
		BorderSizePixel = 0,
		CanvasSize = ui:Computed(function(use)
			return UDim2.fromOffset(0, if use(state.layoutMode) == "wide" then 440 else 700)
		end),
		Position = UDim2.fromOffset(0, 58),
		ScrollBarImageColor3 = ui:Computed(function(use)
			return use(state.theme).TextMuted
		end),
		ScrollBarThickness = 4,
		Size = UDim2.new(1, 0, 1, -88),
		Visible = mainVisible,
		[ui:childrenKey()] = {
			ConnectionSection.create(
				ui,
				state,
				sectionPosition(ui, state, "connection"),
				sectionSize(ui, state, "connection"),
				function()
					controllers.bridge:retry()
				end
			),
			ActivitySection.create(
				ui,
				state,
				sectionPosition(ui, state, "activity"),
				sectionSize(ui, state, "activity")
			),
			StudioSection.create(ui, state, sectionPosition(ui, state, "studio"), sectionSize(ui, state, "studio")),
			SelectionSection.create(
				ui,
				state,
				sectionPosition(ui, state, "selection"),
				sectionSize(ui, state, "selection"),
				function()
					controllers.selection:openInLattice()
				end,
				function()
					controllers.selection:copyReference()
				end
			),
			RecorderSection.create(
				ui,
				state,
				sectionPosition(ui, state, "recorder"),
				sectionSize(ui, state, "recorder"),
				function()
					controllers.recorder:startRecording()
				end,
				function()
					controllers.recorder:stopRecording()
				end,
				function()
					state.markerOpen:set(true)
				end
			),
		},
	})

	local detailView = DetailsSection.create(ui, state, UDim2.fromOffset(8, 66), UDim2.new(1, -16, 1, -104), function()
		state.view:set("main")
	end)
	local detailContainer = ui:New("Frame", {
		Name = "DetailsView",
		BackgroundTransparency = 1,
		Size = UDim2.fromScale(1, 1),
		Visible = detailsVisible,
		[ui:childrenKey()] = detailView,
	})

	local logsView = LogsSection.create(ui, state, UDim2.fromOffset(8, 66), UDim2.new(1, -16, 1, -104), function()
		state.view:set("main")
	end)
	local logsContainer = ui:New("Frame", {
		Name = "LogsView",
		BackgroundTransparency = 1,
		Size = UDim2.fromScale(1, 1),
		Visible = logsVisible,
		[ui:childrenKey()] = logsView,
	})

	local markerInput = ui:New("TextBox", {
		Name = "MarkerInput",
		BackgroundColor3 = ui:Computed(function(use)
			return use(state.theme).SurfaceRaised
		end),
		BorderSizePixel = 0,
		ClearTextOnFocus = false,
		Font = Enum.Font.SourceSans,
		PlaceholderText = "Vehicle started lagging",
		PlaceholderColor3 = ui:Computed(function(use)
			return use(state.theme).TextMuted
		end),
		Position = UDim2.fromOffset(12, 48),
		Size = UDim2.new(1, -24, 0, 30),
		Text = state.markerText,
		TextColor3 = ui:Computed(function(use)
			return use(state.theme).TextPrimary
		end),
		TextSize = 13,
		TextXAlignment = Enum.TextXAlignment.Left,
		[ui:onEventKey("FocusLost")] = function(enterPressed: boolean)
			if enterPressed then
				controllers.recorder:addMarker(ui:peek(state.markerText))
				state.markerText:set("")
				state.markerOpen:set(false)
			end
		end,
		[ui:childrenKey()] = {
			ui:New("UICorner", { CornerRadius = UDim.new(0, 5) }),
			ui:New("UIPadding", {
				PaddingLeft = UDim.new(0, 9),
				PaddingRight = UDim.new(0, 9),
			}),
		},
	})

	local markerDialog = ui:New("Frame", {
		Name = "MarkerDialog",
		AnchorPoint = Vector2.new(0.5, 0.5),
		BackgroundColor3 = ui:Computed(function(use)
			return use(state.theme).Surface
		end),
		BorderSizePixel = 0,
		Position = UDim2.fromScale(0.5, 0.5),
		Size = UDim2.new(1, -32, 0, 128),
		Visible = markerVisible,
		ZIndex = 20,
		[ui:childrenKey()] = {
			ui:New("UICorner", { CornerRadius = UDim.new(0, 6) }),
			ui:New("UIStroke", {
				Color = ui:Computed(function(use)
					return use(state.theme).BorderStrong
				end),
				Thickness = 1,
			}),
			Primitives.label(ui, state, {
				Text = "MARKER",
				ColorToken = "TextMuted",
				Position = UDim2.fromOffset(12, 12),
				Size = UDim2.new(1, -24, 0, 22),
				ZIndex = 21,
			}),
			markerInput,
			Primitives.button(ui, state, {
				Text = "Cancel",
				Position = UDim2.new(1, -150, 1, -37),
				Size = UDim2.fromOffset(62, 25),
				OnActivated = function()
					state.markerOpen:set(false)
				end,
			}),
			Primitives.button(ui, state, {
				Text = "Add Marker",
				Position = UDim2.new(1, -82, 1, -37),
				Size = UDim2.fromOffset(70, 25),
				OnActivated = function()
					controllers.recorder:addMarker(ui:peek(state.markerText))
					state.markerText:set("")
					state.markerOpen:set(false)
				end,
			}),
		},
	})

	ui:New("Frame", {
		Name = "LatticeCockpit",
		BackgroundColor3 = ui:Computed(function(use)
			return use(state.theme).Background
		end),
		BorderSizePixel = 0,
		Size = UDim2.fromScale(1, 1),
		Parent = widget,
		[ui:childrenKey()] = {
			Header.create(ui, state, function()
				state.view:set(if ui:peek(state.view) == "details" then "main" else "details")
			end),
			content,
			detailContainer,
			logsContainer,
			markerDialog,
			Primitives.label(ui, state, {
				Name = "BottomStatus",
				Text = ui:Computed(function(use)
					local errors = use(state.backend).errors or {}
					return if #errors == 0 then "No errors" else "⚠ " .. tostring(#errors) .. " issue(s)"
				end),
				ColorToken = "TextSecondary",
				Position = UDim2.new(0, 12, 1, -28),
				Size = UDim2.new(1, -100, 0, 22),
			}),
			Primitives.button(ui, state, {
				Name = "Logs",
				Text = "Logs",
				Position = UDim2.new(1, -72, 1, -29),
				Size = UDim2.fromOffset(60, 23),
				OnActivated = function()
					state.view:set("logs")
				end,
			}),
		},
	})

	return {
		destroy = function() end,
	}
end

return App
