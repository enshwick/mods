--!strict
-- A player who just joined is screened from their first step: a throw the
-- pack refuses stays refused right after a join, and the pack's
-- first-drink experience comes once. Player B carries at least two Shrine
-- Tonics and has never drunk one in this world.

local PATIENCE = 180
local SETTLE = 3
-- How long after the join the throw may come: a later throw no longer
-- tests the first steps, which the pack refuses anyway.
local THROW_PATIENCE = 60

-- `progress` as one number that only grows: levels, then experience.
local function total(progress: any?): number
	if progress == nil then
		return 0
	end
	return progress.level * 1e9 + progress.experience
end

return {
	about = "a refused throw right after a join stays refused, and a first drink rewards once",
	run = function(s)
		local players = s.players(2, 600)
		local b = players[2]
		s.log(`player B is {b.name}`)

		s.step("B leaves and joins again", function()
			s.tell(`{b.name}: quit to the main menu, then join this world again`)
			s.event("PlayerLeft", PATIENCE, function(data)
				return data.key == b.key
			end)
			local joined = s.event("PlayerJoined", PATIENCE, function(data)
				return data.key == b.key
			end)
			b = enshwick.world.player(joined.entity) or b
		end)
		s.step("a throw within two seconds of arriving is refused", function()
			local b0 = s.items(b.entity)
			s.expect(next(b0) ~= nil, "B carries nothing to throw")
			s.tell(`{b.name}: throw one Shrine Tonic away as soon as you can move`)
			s.event("ItemAction", THROW_PATIENCE, function(action)
				return action.player == b.entity and action.kind == "Trash"
			end)
			s.wait(SETTLE)
			local db = s.changed(b0, s.items(b.entity))
			s.log(`B: {s.describe(db)}`)
			s.equal(next(db), nil, "B's inventory changed")
		end)

		-- The item of B's first drink, which the second must match.
		local tonic: number? = nil
		local function drink(): number
			local before = total(enshwick.world.progress(b.entity))
			s.tell(`{b.name}: drink one Shrine Tonic`)
			local action = s.event("ItemAction", PATIENCE, function(action)
				return action.player == b.entity and action.kind == "Consume"
			end)
			if tonic then
				s.equal(action.item, tonic, "the item drunk")
			end
			tonic = action.item
			s.wait(SETTLE)
			local gained = total(enshwick.world.progress(b.entity)) - before
			s.log(`B gained {gained} for item {action.item}`)
			return gained
		end
		s.step("the first drink gives experience", function()
			s.expect(drink() > 0, "no experience for the first drink")
		end)
		s.step("the second drink of the same tonic gives none", function()
			s.equal(drink(), 0, "experience for the second drink")
		end)
	end,
}
