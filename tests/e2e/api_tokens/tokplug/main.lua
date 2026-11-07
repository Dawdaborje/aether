function ping(input)
    return { pong = true, n = input.n }
end

function other(input)
    return { other = true }
end

-- Announces a won lead; the payload has a field that no webhook may send.
function win(input)
    events.emit({ event = "lead_won", payload = {
        id = input.id, title = input.title, amount = input.amount, status = input.status or "won",
        internal = "never leaves",
    } })
    return { emitted = true }
end

-- An event no webhook can be made for.
function quiet(input)
    events.emit({ event = "internal_ping", payload = { x = 1 } })
    return { emitted = true }
end

-- Inbound hooks: what the kernel hands over after it has checked who is calling.
function on_payment(input)
    return {
        received = true,
        hook = input.hook,
        amount = input.body and input.body.amount,
        raw_length = #input.raw,
        event = input.headers["x-event"],
        saw_signature = input.headers["x-signature"] ~= nil,
        saw_cookie = input.headers["cookie"] ~= nil,
        n = input.query.n,
    }
end

function on_key(input)
    return { received = true, raw = input.raw }
end

function on_signed(input)
    return { received = true, body = input.body }
end

function on_broken(input)
    error("boom: internal detail")
end

-- Imports and exports: thin wrappers, so the checks can drive the kernel commands over HTTP.
function import_start(input)
    return imports.start({
        model = "contact", media_key = input.media_key, storage_key = input.storage_key, mode = input.mode,
        mapping = input.mapping, sheet = input.sheet, stop_on_error = input.stop_on_error, force = input.force,
        on_done = "import_done", ref = input.ref,
    })
end
function import_status(input) return imports.status({ job = input.job }) end
function import_result(input)
    return imports.result({ job = input.job, offset = input.offset, limit = input.limit, only_errors = input.only_errors })
end
function import_undo(input) return imports.undo({ job = input.job }) end
function import_templates(input) return imports.templates({ model = "contact" }) end
function import_guess(input) return imports.guess({ model = "contact", headers = input.headers }) end
function export_contacts(input)
    return exports.run({ model = "contact", filter = input.filter, fields = input.fields, format = input.format, include_id = input.include_id })
end
function read_file(input) return { text = storage.read(input.key) } end

-- Told when an import ends: leaves a memo, so a check can see that it was.
function import_done(input)
    db.create("memo", { body = "import.done:" .. tostring(input.status) .. ":" .. tostring(input.job) .. ":" .. tostring(input.ref) })
    return { ok = true }
end
function memos(input) return db.find("memo", {}) end

function count_contacts(input) return { n = db.count("contact", { filter = {} }) } end
function find_contact(input) return db.find("contact", { filter = { email = input.email } }) end
function rename_contact(input) return db.update("contact", input.id, { name = input.name }) end

function on_mail(input)
  local n = 0
  for _, a in ipairs(input.attachments or {}) do if a.media_key then n = n + 1 end end
  db.create("memo", { body = "mail:" .. input.route.alias .. ":" .. input.subject .. ":" .. input.from.address .. ":" .. n .. ":" .. tostring(input.sender_user) .. ":" .. (input.text or "") })
end
function new_contact(input) return db.create("contact", { name = input.name, email = input.email, owner = input.owner }) end
function mail_token(input) return mail.thread({ model = "contact", record = input.record, alias = input.alias }) end
