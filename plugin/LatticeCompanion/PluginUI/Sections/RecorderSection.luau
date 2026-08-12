--!strict

local Primitives = require(script.Parent.Parent.Components.Primitives)

local RecorderSection = {}

function RecorderSection.create(
	ui: any,
	state: any,
	position: any,
	size: any,
	onStart: () -> (),
	onStop: () -> (),
	onMark: () -> ()
): Frame
	local available = ui:Computed(function(use)
		return use(state.recorder).state ~= "unavailable"
	end)
	local recording = ui:Computed(function(use)
		return use(state.recorder).state == "recording"
	end)
	return Primitives.section(ui, state, "Flight Recorder", position, size, {
		Primitives.label(ui, state, {
			Text = ui:Computed(function(use)
				local recorder = use(state.recorder)
				if recorder.state == "recording" then
					return "● Recording"
				elseif recorder.state == "inactive" then
					return "○ Inactive"
				elseif recorder.state == "failed" then
					return "✕ Failed"
				end
				return "○ Flight Recorder unavailable"
			end),
			Position = UDim2.fromOffset(12, 38),
			Size = UDim2.new(1, -24, 0, 22),
		}),
		Primitives.button(ui, state, {
			Text = "Start Recording",
			Position = UDim2.fromOffset(12, 68),
			Size = UDim2.fromOffset(112, 27),
			Enabled = available,
			Visible = ui:Computed(function(use)
				return not use(recording)
			end),
			OnActivated = onStart,
		}),
		Primitives.button(ui, state, {
			Text = "◆ Mark",
			Position = UDim2.fromOffset(12, 68),
			Size = UDim2.fromOffset(76, 27),
			Enabled = available,
			Visible = recording,
			OnActivated = onMark,
		}),
		Primitives.button(ui, state, {
			Text = "Stop",
			Position = UDim2.fromOffset(96, 68),
			Size = UDim2.fromOffset(64, 27),
			Enabled = available,
			Visible = recording,
			OnActivated = onStop,
		}),
	})
end

return RecorderSection
