--!strict
-- A teleport of the first player's character lands where asked and stays
-- there. Needs one player, on a character of their own.

-- The offset from where the character stands, in metres: aside and a little
-- up, so the character lands on the ground it stood on.
local OFFSET = vector.create(3, 1, 0)
-- How far from the target the character may stand once it has landed.
local SLACK = 1.5

return {
	about = "a teleport of the first player's character lands where asked",
	run = function(s)
		local me = 0
		local target = vector.zero
		local function off(): number
			local at = enshwick.world.position(me)
			s.expect(at, "the character's position does not read")
			return if at then vector.magnitude(at - target) else math.huge
		end
		s.step("the first player's position reads", function()
			me = s.players(1, 300)[1].entity
			local from = enshwick.world.position(me)
			s.expect(from, "the character's position does not read")
			if from then
				s.log(`at {from}`)
				target = from + OFFSET
			end
		end)
		s.step("the character lands at the target", function()
			enshwick.effects.teleport(me, target, {})
			s.wait(1)
			local far = off()
			s.expect(far <= SLACK, `{far} m from the target`)
		end)
		s.step("the character stays at the target", function()
			s.wait(5)
			local far = off()
			s.expect(far <= SLACK, `{far} m from the target after 5 s`)
		end)
	end,
}
