--!strict

local Tokens = require(script.Parent.Parent.Theme.Tokens)

local ThemeController = {}
ThemeController.__index = ThemeController

function ThemeController.new(ui: any, state: any)
	return setmetatable({
		ui = ui,
		state = state,
		connection = nil,
	}, ThemeController)
end

function ThemeController:_refresh()
	self.state.theme:set(Tokens.fromStudio(settings().Studio.Theme))
end

function ThemeController:start()
	self:_refresh()
	self.connection = settings().Studio.ThemeChanged:Connect(function()
		self:_refresh()
	end)
end

function ThemeController:destroy()
	if self.connection ~= nil then
		self.connection:Disconnect()
		self.connection = nil
	end
end

return ThemeController
