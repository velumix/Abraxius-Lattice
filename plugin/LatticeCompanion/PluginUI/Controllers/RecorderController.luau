--!strict

local RecorderController = {}
RecorderController.__index = RecorderController

function RecorderController.new(ui: any, state: any, bridge: any, activity: any)
	return setmetatable({ ui = ui, state = state, bridge = bridge, activity = activity }, RecorderController)
end

function RecorderController:startRecording()
	if self.ui:peek(self.state.recorder).state == "unavailable" then
		self.activity:log("recorder", "Flight Recorder is unavailable")
		return
	end
	self.bridge:enqueueEvent({ kind = "extension", name = "recorder_control_requested", payload = { action = "start" } })
	self.activity:log("recorder", "Recording start requested")
end

function RecorderController:stopRecording()
	if self.ui:peek(self.state.recorder).state == "unavailable" then
		self.activity:log("recorder", "Flight Recorder is unavailable")
		return
	end
	self.bridge:enqueueEvent({ kind = "extension", name = "recorder_control_requested", payload = { action = "stop" } })
	self.activity:log("recorder", "Recording stop requested")
end

function RecorderController:addMarker(text: string)
	if self.ui:peek(self.state.recorder).state ~= "recording" then
		self.activity:log("recorder", "Marker rejected: Flight Recorder is not recording")
		return
	end
	local trimmed = string.match(text, "^%s*(.-)%s*$") or ""
	if #trimmed == 0 or #trimmed > 160 then
		self.activity:log("recorder", "Marker rejected: enter 1–160 characters")
		return
	end
	self.bridge:enqueueEvent({ kind = "extension", name = "recorder_marker_requested", payload = { label = trimmed } })
	self.activity:log("recorder", "Marker requested: " .. trimmed)
end

function RecorderController:destroy() end

return RecorderController
