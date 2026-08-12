--!strict

local HttpService = game:GetService("HttpService")

local Transport = {}

export type Result = {
	succeeded: boolean,
	statusCode: number?,
	body: { [string]: any }?,
	latencyMs: number,
	error: string?,
}

local function request(baseUrl: string, method: string, path: string, token: string?, body: { [string]: any }?): Result
	local started = os.clock()
	local encoded: string? = nil
	if body ~= nil then
		local encodedOk, encodedValue = pcall(function()
			return HttpService:JSONEncode(body)
		end)
		if not encodedOk then
			return {
				succeeded = false,
				latencyMs = math.round((os.clock() - started) * 1000),
				error = "Request serialization failed",
			}
		end
		encoded = encodedValue
	end
	local headers: { [string]: string } = {}
	if token ~= nil then
		headers["Authorization"] = "Bearer " .. token
	end
	if body ~= nil then
		headers["Content-Type"] = "application/json"
	end
	local requestOptions: { [string]: any } = {
		Url = baseUrl .. path,
		Method = method,
		Headers = headers,
	}
	if encoded ~= nil then
		requestOptions.Body = encoded
	end

	local requestOk, response = pcall(function()
		return HttpService:RequestAsync(requestOptions)
	end)
	local latencyMs = math.round((os.clock() - started) * 1000)
	if not requestOk or type(response) ~= "table" then
		return {
			succeeded = false,
			latencyMs = latencyMs,
			error = "Lattice loopback service is unreachable",
		}
	end

	local decoded = nil
	if type(response.Body) == "string" and response.Body ~= "" then
		local decodeOk, value = pcall(function()
			return HttpService:JSONDecode(response.Body)
		end)
		if decodeOk and type(value) == "table" then
			decoded = value
		end
	end
	return {
		succeeded = response.Success == true and decoded ~= nil,
		statusCode = response.StatusCode,
		body = decoded,
		latencyMs = latencyMs,
		error = if response.Success == true then nil else "Lattice rejected the bridge request",
	}
end

function Transport.get(baseUrl: string, path: string): Result
	return request(baseUrl, "GET", path, nil, nil)
end

function Transport.post(baseUrl: string, token: string?, path: string, body: { [string]: any }): Result
	return request(baseUrl, "POST", path, token, body)
end

return Transport
