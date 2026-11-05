--!strict
-- A modifier the probe sets on the first player's maximum health counts
-- toward it as the probe's, and its removal puts the value back. Needs one
-- player.

-- The probe's key for the modifier.
local KEY = "probe-scenario"
-- The extra maximum health.
local EXTRA = 100
-- How long the value takes to follow a change.
local SETTLE = 1

-- Extra maximum health, 0 without a modifier.
local HEALTH_MAX = enshwick.stats.open_stat("enshwick:health_max")
-- Opens the module's handle; until then its functions raise.
local _STATS = enshwick.stats.stats()

local me = 0

return {
	about = "a modifier on a player's maximum health counts as the probe's and goes again",
	cleanup = function()
		if me ~= 0 then
			enshwick.stats.remove(me, KEY, HEALTH_MAX)
		end
	end,
	run = function(s)
		local before = 0
		s.step("the first player's extra maximum health reads", function()
			me = s.players(1, 300)[1].entity
			before = enshwick.stats.value(me, HEALTH_MAX)
			s.log(`extra maximum health {before}`)
		end)
		s.step("the probe's modifier raises it", function()
			enshwick.stats.set(me, KEY, { stat = HEALTH_MAX, op = "Add", value = EXTRA }, "UntilRemoved")
			s.wait(SETTLE)
			s.equal(enshwick.stats.value(me, HEALTH_MAX), before + EXTRA, "the maximum health")
		end)
		s.step("the sources name the probe's modifier", function()
			local found = false
			for _, source in enshwick.stats.sources(me, HEALTH_MAX) do
				s.log(`source {source.op} {source.value}`)
				local origin: any = source.origin
				found = found or (origin.Mod ~= nil and source.value == EXTRA)
			end
			s.expect(found, "no source of the probe's")
		end)
		s.step("its removal puts the value back", function()
			enshwick.stats.remove(me, KEY, HEALTH_MAX)
			s.wait(SETTLE)
			s.equal(enshwick.stats.value(me, HEALTH_MAX), before, "the maximum health")
		end)
	end,
}
