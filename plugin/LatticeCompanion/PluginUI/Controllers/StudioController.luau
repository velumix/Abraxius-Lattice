--!strict

local RunService = game:GetService("RunService")

local StudioController = {}
StudioController.__index = StudioController

local function displayMode(): string
	local runState = RunService.RunState
	if runState == Enum.RunState.Paused then
		return "paused"
	elseif RunService:IsRunning() then
		return "play"
	end
	return "edit"
end

function StudioController.new(state: any, bridge: any)
	return setmetatable({ state = state, bridge = bridge, connection = nil }, StudioController)
end

function StudioController:_refresh()
	local mode = displayMode()
	self.state.studio:set({
		mode = mode,
		placeName = game.Name,
		placeId = if game.PlaceId > 0 then game.PlaceId else nil,
	})
	self.bridge:enqueueEvent({
		kind = "mode_changed",
		data_model = if mode == "edit" then "edit" else "play",
		is_running = RunService:IsRunning(),
	})
end

function StudioController:start()
	self:_refresh()
	self.connection = RunService:GetPropertyChangedSignal("RunState"):Connect(function()
		self:_refresh()
	end)
end

function StudioController:destroy()
	if self.connection ~= nil then
		self.connection:Disconnect()
		self.connection = nil
	end
end

return StudioController
