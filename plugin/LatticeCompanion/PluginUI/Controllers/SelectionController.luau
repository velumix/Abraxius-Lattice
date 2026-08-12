--!strict

local Selection = game:GetService("Selection")

local SelectionController = {}
SelectionController.__index = SelectionController

local function pathComponents(instance: Instance): { string }
	local reversed = {}
	local current: Instance? = instance
	while current ~= nil and current ~= game do
		table.insert(reversed, current.Name)
		current = current.Parent
	end
	local components = {}
	for index = #reversed, 1, -1 do
		table.insert(components, reversed[index])
	end
	return components
end

function SelectionController.new(state: any, bridge: any, activity: any)
	return setmetatable({
		state = state,
		bridge = bridge,
		activity = activity,
		connection = nil,
	}, SelectionController)
end

function SelectionController:_refresh()
	local selected = Selection:Get()
	if #selected == 0 then
		self.bridge:setSelectionSnapshot({ instances = {} })
		self.state.selection:set({
			count = 0,
			name = nil,
			className = nil,
			displayPath = nil,
			pathComponents = {},
			resourceRef = nil,
			resolution = "empty",
			references = nil,
			diagnostics = nil,
		})
		return
	end
	local primary = selected[1]
	local components = pathComponents(primary)
	self.bridge:setSelectionSnapshot({
		instances = {
			{
				display_path = primary:GetFullName(),
				path_components = components,
				class_name = primary.ClassName,
			},
		},
	})
	self.state.selection:set({
		count = #selected,
		name = primary.Name,
		className = primary.ClassName,
		displayPath = primary:GetFullName(),
		pathComponents = components,
		resourceRef = nil,
		resolution = "unresolved",
		references = nil,
		diagnostics = nil,
	})
	self.bridge:enqueueEvent({
		kind = "selection_changed",
		instance_paths = { primary:GetFullName() },
	})
end

function SelectionController:start()
	self:_refresh()
	self.connection = Selection.SelectionChanged:Connect(function()
		self:_refresh()
	end)
end

function SelectionController:copyReference()
	-- Roblox exposes no supported clipboard API to plugins. This remains disabled
	-- until Lattice returns a canonical reference and provides a native action.
	self.activity:log("selection", "Copy reference unavailable: resource is unresolved")
end

function SelectionController:openInLattice()
	self.activity:log("selection", "Open in Lattice unavailable: resource is unresolved")
end

function SelectionController:destroy()
	if self.connection ~= nil then
		self.connection:Disconnect()
		self.connection = nil
	end
end

return SelectionController
