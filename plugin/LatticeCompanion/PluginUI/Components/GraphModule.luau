--!strict

-- Path2D graph renderer for the Lattice cockpit.
--
-- The upstream GraphModule project (boatbomber/GraphModule) predates Roblox's
-- Path2D UI primitive and renders each segment as a collection of Frames. We
-- keep its useful bounded-resolution/downsampling idea, but use the native
-- Path2D curve so the cockpit has one renderer, no per-sample line objects,
-- and Fusion remains the owner of the surrounding UI lifetime.
--
-- Upstream reference: https://github.com/boatbeaker/GraphModule
-- Upstream license: MIT (Copyright (c) 2021 boatbomber).

local GraphModule = {}

local POINT_COUNT = 30

local function maximum(samples: { number }): number
	local result = 0
	for _, value in samples do
		result = math.max(result, if type(value) == "number" then value else 0)
	end
	return result
end

local function sampleAt(samples: { number }, index: number): number
	if #samples == 0 then
		return 0
	end
	if #samples == 1 then
		return samples[1] or 0
	end
	local sourceIndex = math.clamp(math.floor((((index - 1) / (POINT_COUNT - 1)) * (#samples - 1)) + 1.5), 1, #samples)
	return samples[sourceIndex] or 0
end

local function controlPoints(samples: { number }): { any }
	local max = maximum(samples)
	local points = table.create(POINT_COUNT)
	for index = 1, POINT_COUNT do
		local x = 0.025 + (((index - 1) / (POINT_COUNT - 1)) * 0.95)
		local value = sampleAt(samples, index)
		local y = if max <= 0 then 0.82 else 0.82 - ((value / max) * 0.68)

		-- Path2DControlPoint is a Roblox engine datatype. It cannot be assigned
		-- through Fusion props, so the Path2D adapter creates these imperatively.
		points[index] = Path2DControlPoint.new(UDim2.fromScale(x, y))
	end
	return points
end

local function unavailable(ui: any, state: any, position: UDim2, size: UDim2): Frame
	return ui:New("Frame", {
		Name = "ActivityGraphUnavailable",
		BackgroundTransparency = 1,
		Position = position,
		Size = size,
		[ui:childrenKey()] = {
			ui:New("TextLabel", {
				Name = "Path2DUnavailable",
				BackgroundTransparency = 1,
				Font = Enum.Font.RobotoMono,
				Text = "Path2D unavailable",
				TextColor3 = ui:Computed(function(use)
					return use(state.theme).TextMuted
				end),
				TextSize = 10,
				TextXAlignment = Enum.TextXAlignment.Right,
				TextYAlignment = Enum.TextYAlignment.Center,
				Size = UDim2.fromScale(1, 1),
			}),
		},
	}) :: Frame
end

function GraphModule.create(ui: any, state: any, position: UDim2, size: UDim2): Frame
	local container = ui:New("Frame", {
		Name = "ActivityPath2DGraph",
		BackgroundTransparency = 1,
		ClipsDescendants = true,
		Position = position,
		Size = size,
	}) :: Frame

	local ok, path = pcall(Instance.new, "Path2D")
	if not ok or path == nil then
		container:Destroy()
		return unavailable(ui, state, position, size)
	end

	path.Name = "ActivityPath"
	path.Thickness = 1.5
	path.Visible = true
	path.ZIndex = 2
	path.Parent = container
	ui:addCleanup(path)

	local function updateTheme()
		path.Color3 = ui:peek(state.theme).Accent
	end

	local function updateData()
		local samples = ui:peek(state.activitySamples)
		if type(samples) ~= "table" then
			return
		end
		local success, errorMessage = pcall(function()
			path:SetControlPoints(controlPoints(samples))
		end)
		if not success then
			warn("[Lattice] Path2D graph update failed: " .. tostring(errorMessage))
		end
	end

	ui:Observer(state.theme):onBind(updateTheme)
	ui:Observer(state.activitySamples):onBind(updateData)
	return container
end

return GraphModule
