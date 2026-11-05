--!strict
-- A craft reward pays each unit of a request to the player who asked, and
-- only them: two players brew with a pack that rewards each brew with one
-- item, and the scenario compares both inventories before and after each
-- request, once the server has seen that request from that player for that
-- many units. A brew shows as two items up by the units brewed: the brewed
-- item and the reward.

-- How long the players may take for each instruction, and how long the
-- server may take to pay.
local PATIENCE = 180
local SETTLE = 5

-- How many items rose by exactly `units`.
local function rose(delta: { [number]: number }, units: number): number
	local n = 0
	for _, change in delta do
		if change == units then
			n += 1
		end
	end
	return n
end

return {
	about = "a craft reward goes to the player who brewed, once per unit",
	run = function(s)
		local players = s.players(2, 600)
		local a, b = players[1], players[2]
		s.log(`player A is {a.name}, player B is {b.name}`)

		-- Waits until each player of `want` has asked for a craft of the
		-- amount `want` names, in any order.
		local function requests(want: { [number]: number })
			s.event("CraftRecipeEvent", PATIENCE, function(craft)
				if want[craft.crafting_operator_id] == craft.recipe_amount then
					want[craft.crafting_operator_id] = nil
				end
				return next(want) == nil
			end)
		end

		-- Tells `text`, waits for the requests of `want` (a count per
		-- player's character) and a craft per request, and returns how
		-- both inventories changed once the server has had time to pay.
		local function brew(text: string, want: { [number]: number }): ({ [number]: number }, { [number]: number })
			local a0, b0 = s.items(a.entity), s.items(b.entity)
			local crafts = 0
			for _ in want do
				crafts += 1
			end
			s.tell(text)
			requests(want)
			local left = crafts
			s.event("CraftRecipeSuccessEvent", PATIENCE, function()
				left -= 1
				return left == 0
			end)
			s.wait(SETTLE)
			local da, db = s.changed(a0, s.items(a.entity)), s.changed(b0, s.items(b.entity))
			s.log(`A: {s.describe(da)}; B: {s.describe(db)}`)
			return da, db
		end

		s.step("A brews 3 in one request: A gets 3 rewards, B none", function()
			local da, db = brew(`{a.name}: brew 3 Shrine Tonics in one request`, { [a.entity] = 3 })
			s.equal(rose(da, 3), 2, "items of A up by 3 (tonic and reward)")
			s.equal(next(db), nil, "B's inventory changed")
		end)
		s.step("B brews 1: B gets 1 reward, A none", function()
			local da, db = brew(`{b.name}: brew 1 Shrine Tonic`, { [b.entity] = 1 })
			s.equal(rose(db, 1), 2, "items of B up by 1 (tonic and reward)")
			s.equal(next(da), nil, "A's inventory changed")
		end)
		s.step("a refused brew of B pays nothing", function()
			local b0 = s.items(b.entity)
			s.tell(`{b.name}: drop all Night Lilies, then try to brew 1 Shrine Tonic`)
			-- Without the request the step would pass having tested nothing.
			requests({ [b.entity] = 1 })
			s.no_event("CraftRecipeSuccessEvent", SETTLE)
			s.tell(`{b.name}: pick the Night Lilies up again`)
			s.event("ItemAction", PATIENCE, function(action)
				return action.player == b.entity and action.kind == "Pickup"
			end)
			s.wait(SETTLE)
			local db = s.changed(b0, s.items(b.entity))
			s.log(`B: {s.describe(db)}`)
			s.equal(rose(db, 1), 0, "items of B up by 1")
		end)
		s.step("A brews 1 right after: A gets 1 reward, B none", function()
			local da, db = brew(`{a.name}: brew 1 Shrine Tonic now`, { [a.entity] = 1 })
			s.equal(rose(da, 1), 2, "items of A up by 1 (tonic and reward)")
			s.equal(next(db), nil, "B's inventory changed")
		end)
		s.step("both brew 1 at once: each gets 1 reward", function()
			local da, db = brew(
				`{a.name} and {b.name}: brew 1 Shrine Tonic each at the same moment`,
				{ [a.entity] = 1, [b.entity] = 1 }
			)
			local both = rose(da, 1) + rose(db, 1)
			-- The two rewards may swap here; only their number counts.
			s.equal(both, 4, "items up by 1 across A and B (two tonics and two rewards)")
		end)
	end,
}
