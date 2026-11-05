--!strict
-- A quest the first player has not got is granted and taken back again; both
-- read back and come as `KnowledgeChanged`. Needs one player, on a character
-- of their own: what the grant brings, such as experience, stays.

-- How long a change may take to come as an event.
local PATIENCE = 10

local me = 0
local quest = 0

return {
	about = "a quest is granted to a player and taken back, and both read back",
	-- A failed step must not leave the quest granted.
	cleanup = function()
		local known = if quest ~= 0 then enshwick.knowledge.get(me, quest) else nil
		if known and known.value ~= 0 then
			enshwick.knowledge.revoke(me, quest, "Clear")
		end
	end,
	run = function(s)
		local function changed(value: number)
			s.event("KnowledgeChanged", PATIENCE, function(change)
				return change.id == quest and change.entity == me and change.value == value
			end)
			local now = enshwick.knowledge.get(me, quest)
			s.equal(if now then now.value else nil, value, `quest {quest}`)
		end
		s.step("a quest the first player has not got reads", function()
			me = s.players(1, 300)[1].entity
			for _, id in enshwick.knowledge.quests() do
				local known = enshwick.knowledge.get(me, id)
				if known and known.value == 0 then
					quest = id
					break
				end
			end
			s.expect(quest ~= 0, "the player has every quest, or none reads")
			s.log(`quest {quest}`)
		end)
		s.step("the grant reads back", function()
			enshwick.knowledge.grant(me, quest, "Set")
			changed(1)
		end)
		s.step("the revoke reads back", function()
			enshwick.knowledge.revoke(me, quest, "Clear")
			changed(0)
		end)
	end,
}
