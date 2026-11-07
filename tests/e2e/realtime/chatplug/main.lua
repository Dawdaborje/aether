-- A tiny chat: the record of a message is the `message` row; the WebSocket only carries the signal.

local function key(id)
    return (tostring(id):gsub("^[^:]*:", ""))
end

local function me()
    return context.get().actor.id
end

local function is_member(channel, user)
    return #db.find("member", { filter = { channel = channel, user = user }, limit = 1 }) > 0
end

-- Called by the kernel, as the person who asks, before they may join `channel.<id>`.
function can_join(input)
    local channel = db.get("channel", input.params.channel)
    if channel == nil then return { allow = false, reason = "no such channel" } end
    if channel.private ~= true or is_member(input.params.channel, input.user) then
        return { allow = true }
    end
    return { allow = false, reason = "private channel" }
end

function can_inbox(input)
    return { allow = true }
end

function can_online(input)
    return { allow = true }
end

function never(input)
    return { allow = false, reason = "locked" }
end

function make_channel(input)
    local channel = db.create("channel", { name = input.name, private = input.private == true })
    return { id = key(channel.id) }
end

function join_channel(input)
    if not is_member(input.channel, me()) then
        db.create("member", { channel = input.channel, user = me() })
    end
    return { joined = true }
end

function leave_channel(input)
    for _, row in ipairs(db.find("member", { filter = { channel = input.channel, user = me() } })) do
        db.delete("member", key(row.id))
    end
    realtime.drop({ topic = "channel." .. input.channel, user = me() })
    return { left = true }
end

local function next_n(channel)
    local last = db.find("message", { filter = { channel = channel }, order = "-n", limit = 1 })
    return (last[1] and last[1].n or 0) + 1
end

function post_message(input)
    local row = db.create("message", { channel = input.channel, author = me(), author_name = input.as, body = input.body, n = next_n(input.channel), files = input.files })
    local seq = realtime.publish({ topic = "channel." .. input.channel, event = "message", payload = { row = row } })
    realtime.publish({ topic = "inbox", event = "activity", payload = { channel = input.channel } })
    local cursor = db.find("cursor", { filter = { channel = input.channel, user = me() }, limit = 1 })[1]
    if cursor then db.update("cursor", key(cursor.id), { n = row.n }) else db.create("cursor", { channel = input.channel, user = me(), n = row.n }) end
    return { id = key(row.id), n = row.n, seq = seq.seq }
end

-- Announce without a row: the widget must ask `channel_messages` again.
function ping_channel(input)
    return realtime.publish({ topic = "channel." .. input.channel, event = "changed", payload = {} })
end

function who_is_here(input)
    return realtime.presence({ topic = "channel." .. input.channel })
end

-- Oldest first. `before` is the id of the oldest row the caller holds.
function channel_messages(input)
    local limit = input.limit or 50
    local filter = { channel = input.channel }
    if input.before then
        local pivot = db.get("message", key(input.before))
        if pivot then filter.n = { lt = pivot.n } end
    end
    local rows = db.find("message", { filter = filter, order = "-n", limit = limit })
    local out = {}
    for i = #rows, 1, -1 do out[#out + 1] = rows[i] end
    -- Opening the newest page is reading it.
    if not input.before and rows[1] then
        local cursor = db.find("cursor", { filter = { channel = input.channel, user = me() }, limit = 1 })[1]
        if cursor then db.update("cursor", key(cursor.id), { n = rows[1].n }) else db.create("cursor", { channel = input.channel, user = me(), n = rows[1].n }) end
    end
    return { rows = out, more = #rows == limit }
end

-- The rail: the channels the caller may see, grouped, with how many messages they have not read.
function my_channels(input)
    local out = {}
    for _, row in ipairs(db.find("channel", { order = "name", limit = 200 })) do
        local id = key(row.id)
        if row.private ~= true or is_member(id, me()) then
            local cursor = db.find("cursor", { filter = { channel = id, user = me() }, limit = 1 })[1]
            local read = cursor and cursor.n or 0
            local unread = #db.find("message", { filter = { channel = id, n = { gt = read } }, limit = 100 })
            out[#out + 1] = { id = id, name = row.name, group = row.private and "Private" or "Channels", unread = unread }
        end
    end
    return out
end

-- Seeds many messages at once (for the scrolling checks).
function bulk_messages(input)
    local n = next_n(input.channel)
    for i = 1, input.count do
        db.create("message", { channel = input.channel, author = input.author or "seed", body = "seed message " .. (n + i - 1), n = n + i - 1 })
    end
    return { added = input.count }
end

-- The people rail: everyone in the organization; the page shows who is connected.
function my_people(input)
    local out = {}
    for _, member in ipairs(org.members()) do
        out[#out + 1] = { id = key(member.id), name = member.display_name or member.id, user = member.id, group = "People" }
    end
    return out
end

-- Names for the avatars of `<presence>`.
function people(input)
    local wanted = {}
    for _, id in ipairs(input.users or {}) do wanted[key(id)] = true end
    local out = {}
    for _, member in ipairs(org.members()) do
        if wanted[key(member.id)] then out[#out + 1] = { id = member.id, name = member.display_name or member.id } end
    end
    return out
end
