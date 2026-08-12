--!strict
-- Lattice Companion bootstrap. Business logic remains in the native daemon;
-- this script only composes Studio controllers and the Fusion cockpit.

local RunService = game:GetService("RunService")

-- Studio clones plugins into play DataModels. Only Edit owns a bridge/UI.
if RunService:IsRunning() then
	return
end

local Fusion = require(script.Fusion)
local FusionAdapter = require(script.PluginUI.UI.FusionAdapter)
local PluginState = require(script.PluginUI.State.PluginState)
local ThemeController = require(script.PluginUI.Controllers.ThemeController)
local ActivityController = require(script.PluginUI.Controllers.ActivityController)
local StudioController = require(script.PluginUI.Controllers.StudioController)
local SelectionController = require(script.PluginUI.Controllers.SelectionController)
local BridgeController = require(script.PluginUI.Controllers.BridgeController)
local RecorderController = require(script.PluginUI.Controllers.RecorderController)
local App = require(script.PluginUI.App)

local ui = FusionAdapter.new(Fusion)
local state = PluginState.new(ui)
local activity = ActivityController.new(state)
local theme = ThemeController.new(ui, state)
local bridge = BridgeController.new(plugin, state, activity)
local studio = StudioController.new(state, bridge)
local selection = SelectionController.new(state, bridge, activity)
local recorder = RecorderController.new(ui, state, bridge, activity)

theme:start()
local app = App.mount(plugin, ui, state, {
	bridge = bridge,
	selection = selection,
	recorder = recorder,
})

studio:start()
selection:start()
bridge:start()

local unloading = plugin.Unloading:Connect(function()
	bridge:destroy()
	studio:destroy()
	selection:destroy()
	recorder:destroy()
	theme:destroy()
	app:destroy()
	ui:destroy()
end)

ui:addCleanup(unloading)
