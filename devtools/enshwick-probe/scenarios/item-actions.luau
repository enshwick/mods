--!strict
-- What the server sees for each thing a player does with an item, in a
-- hosted world: the player follows the instructions in chat, and the
-- scenario logs the item action and the game events each step raises.
-- The player carries cooked food, a healing potion on the action bar, a
-- piece of armour not worn, a placeable prop and materials for one
-- hand-crafted item.

-- How long the player may take for each instruction.
local PATIENCE = 180

-- Each instruction and the item action it should raise, in order.
local STEPS = {
	{ "open the inventory and eat the food", "Consume" },
	{ "drink the healing potion from the action bar", "Consume" },
	{ "equip the piece of armour", "Equip" },
	{ "unequip it", "Unequip" },
	{ "place the prop", "Place" },
	{ "throw one item out of the inventory", "Trash" },
	{ "pick that item up again", "Pickup" },
}

return {
	about = "each item action a player takes reaches the server as its kind",
	run = function(s)
		local player = s.players(1, 600)[1]
		local function by(kind: string): (any) -> boolean
			return function(action)
				return action.player == player.entity and action.kind == kind
			end
		end
		for _, step in STEPS do
			local instruction, kind = step[1], step[2]
			s.step(`{instruction}: {kind}`, function()
				s.tell(`{player.name}: {instruction}`)
				local action = s.event("ItemAction", PATIENCE, by(kind))
				s.log(`item {action.item} x{action.count} from {action.slot or "no slot"}`)
				s.wait(3)
			end)
		end
		s.step("craft one item by hand: CraftRecipeSuccessEvent", function()
			s.tell(`{player.name}: craft one item from the hand-crafting menu`)
			local craft = s.event("CraftRecipeSuccessEvent", PATIENCE)
			s.log(`recipe {craft.recipe_id} at workshop {craft.workshop_id}`)
		end)
	end,
}
