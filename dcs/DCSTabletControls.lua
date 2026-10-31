-- Dedicated DCS.Tablet profile only. No changes to the aircraft installation.
local lfs = require("lfs")
local root = lfs.writedir():gsub("\\", "/")
if not root:match("/DCS%.Tablet/$") then
	return
end
package.path = package.path .. ";.\\LuaSocket\\?.lua"
package.cpath = package.cpath .. ";.\\LuaSocket\\?.dll"
local socket = require("socket")
local udp, epoch, serial, lastPacket
local buttons, blocked = {}, {}
local ownerPort, previousAircraft
local previous = { start = LuaExportStart, stop = LuaExportStop, frame = LuaExportBeforeNextFrame }
local lastError
local function report(err)
	local text = tostring(err)
	if text ~= lastError and log and log.write then
		pcall(log.write, "DCSTabletControls", log.ERROR, text)
	end
	lastError = text
end
local function action(side, button, value)
	local device = GetDevice(side == 1 and 35 or 36)
	if not device then
		error("DDI device is unavailable")
	end
	device:performClickableAction(3010 + button, value)
end
local function release()
	for key, b in pairs(buttons) do
		local ok, err = pcall(action, b.side, b.button, 0)
		if ok then
			buttons[key] = nil
		else
			report(err)
		end
	end
end
local function apply(left, right, now)
	for side = 1, 2 do
		local mask = side == 1 and left or right
		for button = 1, 20 do
			local key = (side - 1) * 20 + button
			local down = math.floor(mask / 2 ^ (button - 1)) % 2 == 1
			if not down then
				blocked[key] = nil
				if buttons[key] then
					action(side, button, 0)
					buttons[key] = nil
				end
			elseif not buttons[key] and not blocked[key] then
				action(side, button, 1)
				buttons[key] = { side = side, button = button, at = now }
			end
		end
	end
end
local function start()
	if udp then
		udp:close()
	end
	epoch = nil
	serial = nil
	ownerPort = nil
	lastPacket = nil
	previousAircraft = nil
	buttons = {}
	blocked = {}
	local err
	udp, err = socket.udp()
	assert(udp, err)
	local ok
	ok, err = udp:setsockname("127.0.0.1", 17881)
	assert(ok, err)
	udp:settimeout(0)
end
local function stop()
	release()
	if udp then
		udp:close()
		udp = nil
	end
end
local function frame()
	if not udp then
		return
	end
	local now = socket.gettime()
	local self = LoGetSelfData()
	local aircraft = self and self.Name or ""
	if previousAircraft and previousAircraft ~= aircraft then
		release()
		blocked = {}
	end
	previousAircraft = aircraft
	if lastPacket and now - lastPacket > 0.75 then
		for key in pairs(buttons) do
			blocked[key] = true
		end
		release()
		lastPacket = nil
	end
	for key, b in pairs(buttons) do
		if now - b.at >= 2 then
			action(b.side, b.button, 0)
			buttons[key] = nil
			blocked[key] = true
		end
	end
	for packet = 1, 32 do
		local text, host, port = udp:receivefrom(128)
		if not text then
			break
		end
		if text == "HDDI1 PING" and host == "127.0.0.1" then
			udp:sendto("HDDI1 PONG " .. (aircraft == "FA-18C_hornet" and "READY" or "WAIT"), host, port)
		end
		local e, s, l, r = text:match("^HDDI1 (%d+) (%d+) (%d+) (%d+)$")
		local n, left, right = tonumber(s), tonumber(l), tonumber(r)
		if host == "127.0.0.1" and e and #e <= 40 and n and n < 2 ^ 53 and left <= 1048575 and right <= 1048575 then
			-- A new worker must first send all released. This also releases an old worker's keys.
			if e ~= epoch or port ~= ownerPort then
				if left == 0 and right == 0 then
					release()
					blocked = {}
					epoch = e
					serial = nil
					ownerPort = port
				end
			end
			if e == epoch and port == ownerPort and (not serial or n >= serial) then
				local ready = aircraft == "FA-18C_hornet"
				local ok, err = true, nil
				if not serial or n > serial then
					if ready then
						ok, err = pcall(apply, left, right, now)
					else
						release()
					end
					if ok then
						serial = n
					else
						report(err)
						release()
					end
				end
				if ok then
					lastPacket = now
					udp:sendto("HDDI1 " .. e .. " " .. s .. " " .. (ready and "READY" or "WAIT"), host, port)
				end
			end
		end
	end
end
local function chain(own, prior)
	return function()
		local ok, err = pcall(own)
		if not ok then
			report(err)
		end
		if prior then
			local good, why = pcall(prior)
			if not good then
				report(why)
			end
		end
	end
end
LuaExportStart = chain(start, previous.start)
LuaExportStop = chain(stop, previous.stop)
LuaExportBeforeNextFrame = chain(frame, previous.frame)
