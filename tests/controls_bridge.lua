-- Test the actual bridge against mocked DCS and LuaSocket APIs.
local now, aircraft=0,'FA-18C_hornet'
local packets, calls, replies={}, {}, {}
local closed, chained=0,0
local currentRoot='C:/Users/test/Saved Games/DCS.Tablet/'
package.loaded.lfs={writedir=function() return currentRoot end}
local sock={}
function sock:setsockname(host,port) assert(host=='127.0.0.1' and port==17881); return 1 end
function sock:settimeout(n) assert(n==0) end
function sock:receivefrom() local p=table.remove(packets,1); if p then return p[1],p[2] or '127.0.0.1',p[3] or 50000 end end
function sock:sendto(text) replies[#replies+1]=text; return #text end
function sock:close() closed=closed+1 end
package.loaded.socket={udp=function() return sock end,gettime=function() return now end}
function LoGetSelfData() return {Name=aircraft} end
function GetDevice(id) return {performClickableAction=function(_,command,value) calls[#calls+1]={id,command,value} end} end
LuaExportBeforeNextFrame=function() chained=chained+1 end
local function send(serial,left,right,epoch)
 packets[#packets+1]={'HDDI1 '..(epoch or '100')..' '..serial..' '..left..' '..right}
 LuaExportBeforeNextFrame()
end
local function last(id,command,value)
 local c=calls[#calls]; assert(c and c[1]==id and c[2]==command and c[3]==value, 'unexpected cockpit action')
end
dofile('dcs/DCSTabletControls.lua'); LuaExportStart()
send(1,0,0); assert(replies[#replies]=='HDDI1 100 1 READY')
-- All 40 actual command mappings and release transitions.
for side=1,2 do
 for b=1,20 do
  send(2+(side-1)*40+(b-1)*2,side==1 and 2^(b-1) or 0,side==2 and 2^(b-1) or 0)
  last(side==1 and 35 or 36,3010+b,1)
  send(3+(side-1)*40+(b-1)*2,0,0); last(side==1 and 35 or 36,3010+b,0)
 end
end
send(82,1,2); local n=#calls
send(82,1,2); assert(#calls==n,'duplicate pressed twice')
send(81,0,0); assert(#calls==n,'old packet changed state')
-- Loss of contact releases both buttons.
now=now+0.8; LuaExportBeforeNextFrame(); assert(#calls==n+2)
n=#calls;send(82,1,2);assert(#calls==n,'duplicate after timeout reactivated keys')
-- A release snapshot rearms keys after contact loss.
send(83,0,0)
-- A continuous hold ends at two seconds, even with fresh heartbeats.
send(84,1,0)
for i=1,5 do now=now+0.45; send(84+i,1,0) end
last(35,3011,0); n=#calls; send(90,1,0); assert(#calls==n,'expired hold reactivated')
send(91,0,0); send(92,1,0); last(35,3011,1)
-- Invalid packets and a new epoch with a pressed mask are ignored.
n=#calls; send(93,1048576,0); send(1,1,0,'200'); assert(#calls==n)
-- A worker restart first releases old keys.
send(1,0,0,'200'); last(35,3011,0)
aircraft='A-10C'; send(2,1,0,'200'); assert(replies[#replies]=='HDDI1 200 2 WAIT'); n=#calls
assert(chained>80,'prior exporter was not chained')
LuaExportStop(); assert(closed==1)
-- The normal profile must not install callbacks or open a socket.
currentRoot='C:/Users/test/Saved Games/DCS/'
local callback=LuaExportStart; dofile('dcs/DCSTabletControls.lua'); assert(LuaExportStart==callback)
print('PASS: Lua bridge mappings, deduplication, watchdogs, profile guard, and callback chaining')
