--!strict
-- A save held by the probe waits; once let go, a save asked runs
-- `WorldSaving` before `WorldSaved` and the save's information follows it.
-- Needs no player.

return {
	about = "a held save is refused, a released one saves with WorldSaving before WorldSaved",
	-- A failed check must not leave the world unable to save.
	cleanup = function()
		enshwick.world.hold_saves(false)
	end,
	run = function(s)
		local before = enshwick.world.save_info()
		s.step("the save's information reads", function()
			if before then
				s.log(`saved at {before.saved_at}, {before.size} bytes, {before.backups} backups`)
			else
				s.log("no save on disk yet")
			end
		end)
		s.step("a save asked while the probe holds saves is refused", function()
			enshwick.world.hold_saves(true)
			s.wait(1)
			local asked, refused = pcall(enshwick.world.save)
			s.expect(not asked, "the save was not refused")
			s.log(`refused: {refused}`)
			s.expect(string.find(tostring(refused), "refused", 1, true), "the save failed for another reason")
			s.no_event("WorldSaved", 5)
		end)
		s.step("a save asked once let go runs WorldSaving, then WorldSaved", function()
			enshwick.world.hold_saves(false)
			s.wait(1)
			enshwick.world.save()
			s.event("WorldSaving", 30)
			s.event("WorldSaved", 120)
		end)
		s.step("the save's information names the new save", function()
			local after = enshwick.world.save_info()
			s.expect(after, "no save information after the save")
			if before and after then
				s.expect(after.saved_at > before.saved_at, `saved at {after.saved_at}, before {before.saved_at}`)
			end
		end)
	end,
}
