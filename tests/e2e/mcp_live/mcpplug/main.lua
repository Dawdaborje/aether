function greet(input)
    db.create("note", { body = "hello " .. input.name })
    return { greeting = string.rep("hello " .. input.name .. "! ", input.times or 1), notes = db.count("note", { filter = {} }) }
end

function boom(input)
    fail("this command always fails")
end

function seed(input)
    if db.count("note", { filter = {} }) == 0 then
        db.create("note", { body = "seeded" })
    end
    return { seeded = true }
end
