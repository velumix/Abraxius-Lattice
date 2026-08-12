--!strict

-- The only Lattice module that understands Fusion's public constructor shape.
-- Components consume this adapter rather than importing Fusion directly.

export type Adapter = {
	_scope: any,
	_fusion: any,
	New: (Adapter, string, { [any]: any }) -> Instance,
	Value: (Adapter, any) -> any,
	Computed: (Adapter, (use: (any) -> any) -> any) -> any,
	Observer: (Adapter, any) -> any,
	peek: (Adapter, any) -> any,
	childrenKey: (Adapter) -> any,
	onEventKey: (Adapter, string) -> any,
	addCleanup: (Adapter, any) -> (),
	destroy: (Adapter) -> (),
}

local FusionAdapter = {}
FusionAdapter.__index = FusionAdapter

function FusionAdapter.new(Fusion: any): Adapter
	local self = setmetatable({}, FusionAdapter) :: any
	self._fusion = Fusion
	self._scope = Fusion.scoped(Fusion)
	return self
end

function FusionAdapter:New(className: string, props: { [any]: any }): Instance
	return self._scope:New(className)(props)
end

function FusionAdapter:Value(initialValue: any): any
	return self._scope:Value(initialValue)
end

function FusionAdapter:Computed(processor: (use: (any) -> any) -> any): any
	return self._scope:Computed(processor)
end

function FusionAdapter:Observer(state: any): any
	return self._scope:Observer(state)
end

function FusionAdapter:peek(state: any): any
	return self._fusion.peek(state)
end

function FusionAdapter:childrenKey(): any
	return self._fusion.Children
end

function FusionAdapter:onEventKey(eventName: string): any
	return self._fusion.OnEvent(eventName)
end

function FusionAdapter:addCleanup(item: any)
	table.insert(self._scope, item)
end

function FusionAdapter:destroy()
	self._scope:doCleanup()
end

return FusionAdapter
