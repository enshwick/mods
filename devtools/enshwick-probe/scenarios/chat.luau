--!strict
-- The probe's chat filter drops a line of exactly "drop me" and keeps every
-- other line; one player types, the scenario reads what reached the chat.

local function typed(text: string): (any) -> boolean
	return function(chat)
		return chat.kind == "Player" and chat.text == text
	end
end

return {
	about = 'the chat filter drops a line of exactly "drop me" and every other line reaches the chat',
	run = function(s)
		local player = s.players(1, 600)[1]
		s.step('a line of exactly "drop me" never reaches the chat', function()
			s.tell(`{player.name}: type "drop me", then "hello probe"`)
			local first = s.event("ChatSaid", 300, function(chat)
				return typed("drop me")(chat) or typed("hello probe")(chat)
			end)
			s.equal(first.text, "hello probe", "the first of the two lines heard")
		end)
		s.step("a line holding more than drop me stays", function()
			s.tell(`{player.name}: type "drop me please"`)
			s.event("ChatSaid", 300, typed("drop me please"))
		end)
		s.step("a line names its sender's account and character", function()
			s.tell(`{player.name}: type "who am I"`)
			local chat = s.event("ChatSaid", 300, typed("who am I"))
			s.equal(chat.player, player.key, "the sender's account")
			s.equal(chat.entity, player.entity, "the sender's character")
		end)
	end,
}
