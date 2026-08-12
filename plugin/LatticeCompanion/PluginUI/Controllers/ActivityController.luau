--!strict

local MAX_ACTIVITY = 100
local MAX_LOGS = 200
local SAMPLE_COUNT = 30

local ActivityController = {}
ActivityController.__index = ActivityController

local function appendBounded(items: { any }, item: any, limit: number): { any }
	local nextItems = table.clone(items)
	table.insert(nextItems, 1, item)
	while #nextItems > limit do
		table.remove(nextItems)
	end
	return nextItems
end

function ActivityController.new(state: any)
	return setmetatable({
		state = state,
		requestCount = 0,
		activityItems = {},
		logItems = {},
		samples = table.create(SAMPLE_COUNT, 0),
	}, ActivityController)
end

function ActivityController:record(
	name: string,
	semantics: string,
	status: string,
	durationMs: number?,
	target: string?
)
	local activity = {
		id = tostring(DateTime.now().UnixTimestampMillis) .. ":" .. tostring(#self.activityItems + 1),
		name = name,
		semantics = semantics,
		status = status,
		durationMs = durationMs,
		target = target,
		unixMs = DateTime.now().UnixTimestampMillis,
	}
	self.activityItems = appendBounded(self.activityItems, activity, MAX_ACTIVITY)
	self.state.activity:set(self.activityItems)
end

function ActivityController:log(kind: string, message: string)
	local entry = {
		unixMs = DateTime.now().UnixTimestampMillis,
		kind = kind,
		message = message,
	}
	self.logItems = appendBounded(self.logItems, entry, MAX_LOGS)
	self.state.logs:set(self.logItems)
end

function ActivityController:requestObserved()
	self.requestCount += 1
end

function ActivityController:commitSample()
	local nextSamples = table.clone(self.samples)
	table.insert(nextSamples, self.requestCount)
	while #nextSamples > SAMPLE_COUNT do
		table.remove(nextSamples, 1)
	end
	self.requestCount = 0
	self.samples = nextSamples
	self.state.activitySamples:set(nextSamples)
end

return ActivityController
