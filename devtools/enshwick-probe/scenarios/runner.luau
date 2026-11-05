--!strict
-- The probe's scenario runner: once a world has loaded, it runs the scenario
-- the world's `scenario` setting names, one step after another, and logs a
-- line per step and a last line `scenario <name>: PASS` or `FAIL`.

local scenarios = require("./index")

-- Seconds after the world has loaded before the scenario starts, as the
-- probe's own checks wait.
local START_DELAY = 10

type Waiter = { co: thread, test: ((any) -> boolean)?, done: boolean }

local waiting: { [string]: { Waiter } } = {}
local running: { name: string, co: thread, step: number, title: string?, cleanup: (() -> ())? }? = nil

local function line(text: string)
	enshwick.log.info(text)
end

local function finish(passed: boolean, why: any?)
	local run = running
	if run == nil then
		return
	end
	running = nil
	if run.cleanup then
		local ok, failed = pcall(run.cleanup)
		if not ok then
			passed, why = false, `its cleanup: {failed}`
		end
	end
	if passed then
		line(`scenario {run.name}: PASS, {run.step} steps`)
	else
		local at = if run.title then `step {run.step} {run.title}` else "before the first step"
		enshwick.log.warn(`scenario {run.name}: FAIL at {at}: {tostring(why)}`)
	end
end

-- Resumes the scenario's thread and reports it once it ends.
local function resume(co: thread, ...: any)
	local run = running
	if run == nil or run.co ~= co then
		return
	end
	local ok, why = coroutine.resume(co, ...)
	if not ok then
		finish(false, why)
	elseif coroutine.status(co) == "dead" then
		finish(true)
	end
end

-- Hands `data` to every waiter of `event` whose test it meets.
local function notify(event: string, data: any)
	local list = waiting[event]
	if list == nil then
		return
	end
	waiting[event] = nil
	local keep = {}
	for _, waiter in list do
		if waiter.done then
			continue
		end
		local ok, met = true, true
		if waiter.test then
			ok, met = pcall(waiter.test, data)
		end
		if not ok then
			-- The test's error belongs to the scenario, not to this event.
			waiter.done = true
			resume(waiter.co, "error", met)
		elseif met then
			waiter.done = true
			resume(waiter.co, true, data)
		else
			table.insert(keep, waiter)
		end
	end
	local added = waiting[event]
	if added then
		table.move(added, 1, #added, #keep + 1, keep)
	end
	if #keep > 0 then
		waiting[event] = keep
	end
end

-- The runner's clock, in seconds, moved by the time `Ticked` reports, so
-- every wait counts the server's steps as they come.
local clock = 0
local timers: { { at: number, run: () -> () } } = {}

-- Runs `run` once `seconds` have passed on the runner's clock.
local function after(seconds: number, run: () -> ())
	table.insert(timers, { at = clock + seconds, run = run })
end

enshwick.on("Ticked", function(data)
	clock += (tonumber(data.elapsed_nanos) or 0) / 1e9
	local due = {}
	for i = #timers, 1, -1 do
		if timers[i].at <= clock then
			table.insert(due, 1, table.remove(timers, i))
		end
	end
	for _, timer in due do
		timer.run()
	end
	notify("Ticked", data)
end)

-- Events a scenario may wait for, each heard from the start.
local LOADER_EVENTS = {
	"WorldSaving",
	"WorldSaved",
	"PlayerJoined",
	"PlayerLeft",
	"PlayerDied",
	"DayStarted",
	"NightStarted",
	"KnowledgeChanged",
	"BuildingPlaced",
	"BuildingRemoved",
}
-- Typed game events, heard only where the build has them: a subscription
-- to one it lacks would turn the probe off once the start is over.
local GAME_EVENTS = {
	"CraftRecipeEvent",
	"CraftRecipeSuccessEvent",
	"LootInteractionEvent",
	"HandleEquipmentSlotChangedEvent",
	"BuildingPlaceEvent",
	"BuildingPickUpEvent",
	"XpGainedEvent",
	"LevelUpEvent",
}

-- The reflected name of a typed game event: `CraftRecipeEvent` is
-- `craft_recipe_event`.
local function reflected(event: string): string
	return (event:gsub("(%l)(%u)", "%1_%2"):lower())
end

local heard: { [string]: boolean } = {}
local function hear(event: string)
	enshwick.on(event :: any, function(data: any)
		notify(event, data)
	end)
	heard[event] = true
end
for _, event in LOADER_EVENTS do
	hear(event)
end
heard.Ticked = true
local types: { [string]: boolean } = {}
for _, ty in enshwick.game_event_types() do
	types[ty.name] = true
end
for _, event in GAME_EVENTS do
	if types[reflected(event)] then
		hear(event)
	else
		enshwick.log.info(`scenario runner: {event} is not heard: this build has no {reflected(event)}`)
	end
end
local HOOKS: { [string]: () -> () } = {
	ChatSaid = function()
		enshwick.chat.on_chat_said(function(chat)
			notify("ChatSaid", chat)
		end)
	end,
	-- Registered before the probe's own filter, so it sees the lines that
	-- filter drops.
	ChatTyped = function()
		enshwick.chat.on_chat(function(chat)
			notify("ChatTyped", chat)
			return nil
		end)
	end,
	ItemAction = function()
		enshwick.items.on_item_action(function(action)
			notify("ItemAction", action)
			return nil
		end)
	end,
	FrontChanged = function()
		enshwick.weather.on_front(function(change)
			notify("FrontChanged", change)
		end)
	end,
}
for event, hook in HOOKS do
	local ok, why = pcall(hook)
	heard[event] = ok
	if not ok then
		enshwick.log.warn(`scenario runner: {event} is not heard: {why}`)
	end
end

-- The calls a scenario makes; each wait suspends it until its time or
-- event comes, and every failed check ends it with `FAIL`.
local function context(name: string)
	local s = {}

	local function current(): thread
		local run = running
		assert(run and run.co == coroutine.running(), "a scenario call outside its scenario")
		return run.co
	end

	--- Logs `text` beside the scenario's lines.
	function s.log(text: string)
		line(`scenario {name}: {text}`)
	end

	--- Runs `body` as the next step named `title`; its `PASS` line follows
	--- once it returns.
	function s.step(title: string, body: () -> ())
		local run = running :: any
		run.step += 1
		run.title = title
		body()
		line(`scenario {name}: step {run.step} {title}: PASS`)
	end

	--- Fails the step with `what` unless `ok`.
	function s.expect(ok: any, what: string)
		if not ok then
			error(what, 2)
		end
	end

	--- Fails the step unless `got` equals `want`, naming both.
	function s.equal(got: any, want: any, what: string)
		if got ~= want then
			error(`{what}: expected {tostring(want)}, got {tostring(got)}`, 2)
		end
	end

	--- Waits `seconds` of the server's time.
	function s.wait(seconds: number)
		local co = current()
		after(seconds, function()
			resume(co)
		end)
		coroutine.yield()
	end

	--- Waits for the event `event` whose data meets `test`, at most
	--- `seconds`; its data, `true` for an event without data, or `nil` when
	--- none came in time.
	function s.next(event: string, seconds: number, test: ((any) -> boolean)?): any?
		assert(heard[event], `the runner does not hear {event}`)
		local co = current()
		local waiter: Waiter = { co = co, test = test, done = false }
		local list = waiting[event] or {}
		table.insert(list, waiter)
		waiting[event] = list
		after(seconds, function()
			if not waiter.done then
				waiter.done = true
				resume(co, false)
			end
		end)
		local came, data = coroutine.yield()
		if came == "error" then
			error(`the test of {event}: {data}`, 0)
		elseif not came then
			return nil
		end
		return if data == nil then true else data
	end

	--- Waits for `event` as `next` does and fails the step without it.
	function s.event(event: string, seconds: number, test: ((any) -> boolean)?): any
		local data = s.next(event, seconds, test)
		if data == nil then
			error(`no {event} within {seconds} s`, 2)
		end
		return data
	end

	--- Fails the step if `event` meeting `test` comes within `seconds`.
	function s.no_event(event: string, seconds: number, test: ((any) -> boolean)?)
		local data = s.next(event, seconds, test)
		if data ~= nil then
			error(`{event} came within {seconds} s`, 2)
		end
	end

	--- Waits `steps` server steps, as `Ticked` counts them.
	function s.ticks(steps: number)
		local left = steps
		s.event("Ticked", 5 + steps, function(data)
			left -= tonumber(data.elapsed_steps) or 0
			return left <= 0
		end)
	end

	--- Asks the players to do something, in chat and in the log.
	function s.tell(text: string)
		enshwick.world.broadcast(`[probe] {text}`)
		line(`scenario {name}: told players: {text}`)
	end

	--- The players online once at least `count` are, waiting at most
	--- `seconds`; fails the step without them.
	function s.players(count: number, seconds: number): { any }
		local waited = 0
		while #enshwick.world.players() < count do
			if waited >= seconds then
				error(`fewer than {count} players online after {seconds} s`, 2)
			end
			s.wait(1)
			waited += 1
		end
		return enshwick.world.players()
	end

	local function outcome(call: (number, number, number, (any) -> ()) -> (), root: number, item: number, count: number): any
		local co = current()
		local suspended, result = false, nil
		call(root, item, count, function(done)
			-- The fake host answers within the call, before the scenario
			-- suspends; the game a step later.
			if suspended then
				resume(co, done)
			else
				result = done
			end
		end)
		if result == nil then
			suspended = true
			result = coroutine.yield()
		end
		if result.done ~= "Committed" then
			error(`inventory {root}, item {item} x{count}: {result.done}`, 3)
		end
		return result
	end

	--- Gives `count` of `item` to the inventory `root`; fails the step
	--- unless the game committed it.
	function s.give(root: number, item: number, count: number)
		outcome(enshwick.inventory.give, root, item, count)
	end

	--- Takes `count` of `item` from the inventory `root`; fails the step
	--- unless the game committed it.
	function s.take(root: number, item: number, count: number)
		outcome(enshwick.inventory.take, root, item, count)
	end

	--- What the inventory `root` holds now: a count per item; fails the
	--- step when it reads no slots, so a failed read never passes as empty.
	function s.items(root: number): { [number]: number }
		local slots = enshwick.inventory.slots(root)
		if #slots == 0 then
			error(`inventory {root} reads no slots`, 2)
		end
		local held: { [number]: number } = {}
		for _, slot in slots do
			if slot.item ~= 0 then
				held[slot.item] = (held[slot.item] or 0) + slot.count
			end
		end
		return held
	end

	--- The counts that changed from `before` to `after`, per item.
	function s.changed(before: { [number]: number }, after: { [number]: number }): { [number]: number }
		local delta: { [number]: number } = {}
		for item, count in after do
			if count ~= (before[item] or 0) then
				delta[item] = count - (before[item] or 0)
			end
		end
		for item, count in before do
			if after[item] == nil then
				delta[item] = -count
			end
		end
		return delta
	end

	--- `changed` as a line for the log, items in id order.
	function s.describe(delta: { [number]: number }): string
		local items = {}
		for item in delta do
			table.insert(items, item)
		end
		table.sort(items)
		local parts = {}
		for _, item in items do
			table.insert(parts, `{item} {if delta[item] > 0 then "+" else ""}{delta[item]}`)
		end
		return if #parts == 0 then "nothing" else table.concat(parts, ", ")
	end

	return s
end

local function start(name: string)
	local scenario = scenarios[name]
	if scenario == nil then
		enshwick.log.warn(`scenario {name}: FAIL: no scenario of that name`)
		return
	end
	if running then
		enshwick.log.warn(`scenario {name}: not started, {running.name} still runs`)
		return
	end
	local s = context(name)
	local co = coroutine.create(function()
		scenario.run(s)
	end)
	running = { name = name, co = co, step = 0, title = nil, cleanup = scenario.cleanup }
	line(`scenario {name}: start: {scenario.about}`)
	resume(co)
end

-- Counts world loads, so a start timer of an earlier world never fires.
local loads = 0

enshwick.on("WorldLoaded", function()
	-- A hosted game loads worlds one after another; a scenario left
	-- waiting in the last one would hold every later start.
	finish(false, "the world loaded again before it ended")
	loads += 1
	local load = loads
	local name = enshwick.settings().scenario
	if type(name) ~= "string" or name == "" then
		return
	end
	line(`scenario {name}: starts in {START_DELAY} s`)
	after(START_DELAY, function()
		if load == loads then
			start(name)
		end
	end)
end)
