--!strict
-- The test content mod's rain: a copy of the game's rain in the first
-- weather layer, picked with probability 1. A front counts only when its
-- phenomenon is the copy's own GUID. Needs the test content mod on the
-- server, and no player.

-- How long the layer may take to pick its next front.
local PATIENCE = 900

-- The GUID of the test content mod's rain, from its content id.
local RAIN = string.lower(enshwick.content.guid("enshwick.test-content:rain"))

local function copy(front: any): boolean
	return front.layer == 0 and front.phenomenon ~= nil and string.lower(front.phenomenon) == RAIN
end

return {
	about = "the test content mod's rain comes as a front in the first weather layer",
	run = function(s)
		local function seen(front: any): string
			return `layer {front.layer}, {front.kind}, phenomenon {front.phenomenon}`
		end
		s.step("a rain front runs in the first weather layer", function()
			s.log(`looking for phenomenon {RAIN}`)
			for _, front in enshwick.weather.fronts() do
				s.log(`running: {seen(front)}`)
				if copy(front) then
					s.log(`already running: phenomenon {front.phenomenon}, radius {front.radius} m, {front.remaining} s left`)
					return
				end
			end
			local change = s.event("FrontChanged", PATIENCE, function(change)
				if change.started and not copy(change.front) then
					s.log(`started, not the copy: {seen(change.front)}`)
				end
				return change.started and copy(change.front)
			end)
			local front = change.front
			s.log(`started: phenomenon {front.phenomenon}, radius {front.radius} m, {front.remaining} s left`)
		end)
	end,
}
