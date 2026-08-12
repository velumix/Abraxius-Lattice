--!strict

local HttpService = game:GetService("HttpService")
local RunService = game:GetService("RunService")

local Transport = require(script.Parent.Transport)

local PROTOCOL_VERSION = 1
local PLUGIN_VERSION = "0.2.1"
local DEFAULT_URL = "http://127.0.0.1:13471"
local URL_SETTING = "lattice.bridge.url"
local MAX_EVENTS = 2048
local MAX_RESULTS = 256

local BridgeController = {}
BridgeController.__index = BridgeController

local function studioMode(): string
	if RunService:IsClient() then
		return "client"
	elseif RunService:IsServer() and RunService:IsRunning() then
		return "server"
	elseif RunService:IsRunning() then
		return "play"
	end
	return "edit"
end

local function boundedInsert(queue: { any }, value: any, maximum: number)
	table.insert(queue, value)
	while #queue > maximum do
		table.remove(queue, 1)
	end
end

function BridgeController.new(plugin: Plugin, state: any, activity: any)
	local self = setmetatable({
		plugin = plugin,
		state = state,
		activity = activity,
		externalSessionId = HttpService:GenerateGUID(false),
		bridgeSessionId = nil,
		sessionToken = nil,
		sessionTokenExpiresUnixMs = nil,
		reportSequence = 0,
		eventSequence = 0,
		queuedEvents = {},
		queuedResults = {},
		pendingReport = nil,
		resultCache = {},
		resultOrder = {},
		selectionSnapshot = { instances = {} },
		running = false,
		backoffSeconds = 1,
		reportIntervalSeconds = 0.5,
	}, BridgeController)
	return self
end

function BridgeController:setSelectionSnapshot(snapshot: { [string]: any })
	self.selectionSnapshot = snapshot
end

function BridgeController:_baseUrl(): string
	local configured = self.plugin:GetSetting(URL_SETTING)
	if type(configured) == "string" and string.match(configured, "^http://(localhost|127%.0%.0%.1):%d+$") then
		return configured
	end
	return DEFAULT_URL
end

function BridgeController:_setPairing(status: string, label: string, errorCode: string?, errorMessage: string?)
	self.state.pairing:set({
		status = status,
		label = label,
		errorCode = errorCode,
		errorMessage = errorMessage,
	})
end

function BridgeController:_clearPairing()
	self.sessionToken = nil
	self.sessionTokenExpiresUnixMs = nil
	self.bridgeSessionId = nil
	self.pendingReport = nil
end

function BridgeController:retry()
	self:_clearPairing()
	self:_setPairing("discovering", "Discovering local Lattice", nil, nil)
	self.backoffSeconds = 1
	self.state.connection:set({
		status = "connecting",
		label = "Connecting",
		latencyMs = nil,
		lastSuccessUnixMs = nil,
		errorCode = nil,
		errorMessage = nil,
	})
end

function BridgeController:_pair(): boolean
	self:_setPairing("discovering", "Discovering local Lattice", nil, nil)
	local baseUrl = self:_baseUrl()
	local discovery = Transport.get(baseUrl, "/v1/studio-bridge/discover")
	if not discovery.succeeded or discovery.body == nil then
		self:_setPairing("unavailable", "Lattice unavailable", "STUDIO_BRIDGE_DISCOVERY_FAILED", discovery.error)
		return false
	end
	local challenge = discovery.body.challenge
	if type(challenge) ~= "string" or challenge == "" then
		self:_setPairing(
			"error",
			"Invalid bridge discovery",
			"STUDIO_BRIDGE_DISCOVERY_INVALID",
			"The local bridge returned no pairing challenge"
		)
		return false
	end
	self:_setPairing("pairing", "Pairing with local Lattice", nil, nil)
	local pairing = Transport.post(baseUrl, nil, "/v1/studio-bridge/pair", {
		challenge = challenge,
		client_kind = "roblox-studio-plugin",
		client_name = "LatticeCompanion",
	})
	if not pairing.succeeded or pairing.body == nil then
		self:_setPairing("error", "Local pairing failed", "STUDIO_BRIDGE_PAIRING_FAILED", pairing.error)
		return false
	end
	local token = pairing.body.session_token
	if type(token) ~= "string" or token == "" then
		self:_setPairing(
			"error",
			"Invalid pairing response",
			"STUDIO_BRIDGE_PAIRING_INVALID",
			"The local bridge returned no session credential"
		)
		return false
	end
	self.sessionToken = token
	self.sessionTokenExpiresUnixMs = pairing.body.session_expires_unix_ms
	self.bridgeSessionId = nil
	self.pendingReport = nil
	self:_setPairing("paired", "Paired automatically", nil, nil)
	self.activity:log("connection", "Studio paired with local Lattice")
	return true
end

function BridgeController:enqueueEvent(payload: { [string]: any })
	self.eventSequence += 1
	boundedInsert(self.queuedEvents, {
		event_id = self.externalSessionId .. ":" .. tostring(self.eventSequence),
		sequence = self.eventSequence,
		observed_unix_ms = DateTime.now().UnixTimestampMillis,
		payload = payload,
	}, MAX_EVENTS)
end

function BridgeController:_applyBackend(backend: any)
	if type(backend) == "table" then
		self.state.backend:set(backend)
		if type(backend.recorder) == "table" then
			self.state.recorder:set(backend.recorder)
		end
	end
end

function BridgeController:_request(path: string, body: { [string]: any }): any
	if self.sessionToken == nil then
		return nil
	end
	local result = Transport.post(self:_baseUrl(), self.sessionToken, path, body)
	self.activity:requestObserved()
	if result.succeeded and result.body ~= nil then
		self.state.connection:set({
			status = "connected",
			label = "Connected",
			latencyMs = result.latencyMs,
			lastSuccessUnixMs = DateTime.now().UnixTimestampMillis,
			errorCode = nil,
			errorMessage = nil,
		})
		self:_applyBackend(result.body.backend)
		return result.body
	end
	if result.statusCode == 401 then
		self:_clearPairing()
		self:_setPairing("discovering", "Discovering local Lattice", nil, nil)
	end
	local code = if result.statusCode == 401 then "STUDIO_BRIDGE_SESSION_EXPIRED" else "LATTICE_UNREACHABLE"
	self.state.connection:set({
		status = if result.statusCode == 401 then "error" else "offline",
		label = if result.statusCode == 401 then "Authentication failed" else "Lattice unavailable",
		latencyMs = result.latencyMs,
		lastSuccessUnixMs = nil,
		errorCode = code,
		errorMessage = result.error,
	})
	return nil
end

function BridgeController:_register(): boolean
	self.state.connection:set({
		status = "connecting",
		label = "Connecting",
		latencyMs = nil,
		lastSuccessUnixMs = nil,
		errorCode = nil,
		errorMessage = nil,
	})
	local response = self:_request("/v1/studio-bridge/register", {
		protocol_version = PROTOCOL_VERSION,
		external_session_id = self.externalSessionId,
		plugin_version = PLUGIN_VERSION,
		data_model = studioMode(),
		place_id = if game.PlaceId > 0 then game.PlaceId else nil,
		universe_id = if game.GameId > 0 then game.GameId else nil,
		is_running = RunService:IsRunning(),
	})
	if response == nil or type(response.session_id) ~= "string" then
		return false
	end
	self.bridgeSessionId = response.session_id
	if type(response.report_interval_ms) == "number" then
		self.reportIntervalSeconds = math.clamp(response.report_interval_ms / 1000, 0.2, 15)
	end
	self.activity:log("connection", "Studio registered with Lattice")
	return true
end

function BridgeController:_executeCommand(command: any): { [string]: any }
	local commandId = command.id
	if type(commandId) ~= "string" then
		return {
			command_id = "invalid",
			succeeded = false,
			error_code = "COMMAND_INVALID",
			error_message = "Command ID missing",
		}
	end
	if self.resultCache[commandId] ~= nil then
		return self.resultCache[commandId]
	end

	local started = os.clock()
	local payload = command.payload
	local operationName = if type(payload) == "table" and type(payload.kind) == "string"
		then payload.kind
		else "unknown"
	local result
	if type(payload) ~= "table" or type(payload.kind) ~= "string" then
		result = {
			command_id = commandId,
			succeeded = false,
			error_code = "COMMAND_INVALID",
			error_message = "Command payload missing",
		}
	elseif payload.kind == "get_studio_state" then
		result = {
			command_id = commandId,
			succeeded = true,
			value = {
				data_model = studioMode(),
				is_running = RunService:IsRunning(),
				place_id = if game.PlaceId > 0 then game.PlaceId else nil,
				universe_id = if game.GameId > 0 then game.GameId else nil,
			},
		}
	elseif payload.kind == "get_selection" then
		result = {
			command_id = commandId,
			succeeded = true,
			value = self.selectionSnapshot,
		}
	else
		result = {
			command_id = commandId,
			succeeded = false,
			error_code = "COMMAND_UNAVAILABLE",
			error_message = "Command is not implemented by the cockpit controller",
		}
	end
	local elapsed = math.round((os.clock() - started) * 1000)
	self.activity:record(operationName, "read", if result.succeeded then "succeeded" else "failed", elapsed, nil)
	self.resultCache[commandId] = result
	table.insert(self.resultOrder, commandId)
	while #self.resultOrder > MAX_RESULTS do
		local expired = table.remove(self.resultOrder, 1)
		self.resultCache[expired] = nil
	end
	return result
end

function BridgeController:_buildReport(): { [string]: any }?
	if self.bridgeSessionId == nil then
		return nil
	end
	if self.pendingReport ~= nil then
		return self.pendingReport
	end
	self.reportSequence += 1
	self.pendingReport = {
		protocol_version = PROTOCOL_VERSION,
		session_id = self.bridgeSessionId,
		report_sequence = self.reportSequence,
		data_model = studioMode(),
		place_id = if game.PlaceId > 0 then game.PlaceId else nil,
		universe_id = if game.GameId > 0 then game.GameId else nil,
		is_running = RunService:IsRunning(),
		events = self.queuedEvents,
		command_results = self.queuedResults,
	}
	self.queuedEvents = {}
	self.queuedResults = {}
	return self.pendingReport
end

function BridgeController:_report(): boolean
	local report = self:_buildReport()
	if report == nil then
		return false
	end
	local response = self:_request("/v1/studio-bridge/report", report)
	if response == nil then
		return false
	end
	self.pendingReport = nil
	if type(response.commands) == "table" then
		for _, command in response.commands do
			if type(command) == "table" then
				table.insert(self.queuedResults, self:_executeCommand(command))
			end
		end
	end
	return true
end

function BridgeController:start()
	if self.running then
		return
	end
	self.running = true
	self:_setPairing("discovering", "Discovering local Lattice", nil, nil)
	task.spawn(function()
		local sampleElapsed = 0
		while self.running do
			local expires = self.sessionTokenExpiresUnixMs
			if
				self.sessionToken == nil
				or (type(expires) == "number" and expires <= DateTime.now().UnixTimestampMillis + 5_000)
			then
				self:_clearPairing()
				if not self:_pair() then
					self.state.connection:set({
						status = "offline",
						label = "Lattice unavailable",
						latencyMs = nil,
						lastSuccessUnixMs = nil,
						errorCode = "STUDIO_BRIDGE_DISCOVERY_FAILED",
						errorMessage = "Lattice could not be discovered on the local bridge",
					})
					task.wait(self.backoffSeconds)
					self.backoffSeconds = math.min(self.backoffSeconds * 2, 15)
					continue
				end
			end
			if self.bridgeSessionId == nil and not self:_register() then
				task.wait(self.backoffSeconds)
				self.backoffSeconds = math.min(self.backoffSeconds * 2, 15)
				continue
			end
			self:enqueueEvent({ kind = "heartbeat" })
			if self:_report() then
				self.backoffSeconds = 1
			else
				self.backoffSeconds = math.min(self.backoffSeconds * 2, 15)
			end
			sampleElapsed += self.reportIntervalSeconds
			if sampleElapsed >= 1 then
				self.activity:commitSample()
				sampleElapsed = 0
			end
			task.wait(if self.backoffSeconds > 1 then self.backoffSeconds else self.reportIntervalSeconds)
		end
	end)
end

function BridgeController:destroy()
	self.running = false
end

return BridgeController
