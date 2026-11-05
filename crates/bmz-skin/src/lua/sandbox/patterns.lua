-- Lua 5.4 byte-pattern operations implemented in Lua so the VM instruction
-- hook also meters searching, backtracking, balanced matches and replacements.
-- Keep only bounded native primitives; never retain the native pattern APIs.
local byte, sub = string.byte, string.sub
local unpack, concat = table.unpack, table.concat
local kind, number, integer, str, raise = type, tonumber, math.tointeger, tostring, error
local special = {[94]=true, [36]=true, [42]=true, [43]=true, [63]=true,
    [46]=true, [40]=true, [91]=true, [37]=true, [45]=true}

local function text(value)
    local t = kind(value)
    if t == 'string' then return value end
    if t == 'number' then return str(value) end
    raise('string expected', 3)
end

local function int(value, default)
    if value == nil then return default end
    local n = integer(number(value))
    if not n then raise('integer expected', 3) end
    return n
end

local function startpos(s, init)
    local i = int(init, 1)
    if i < 0 then i = #s + i + 1 end
    if i < 1 then i = 1 end
    return i
end

local function class(c, p)
    local lower = p
    if p >= 65 and p <= 90 then lower = p + 32 end
    local alpha = (c >= 65 and c <= 90) or (c >= 97 and c <= 122)
    local digit = c >= 48 and c <= 57
    local yes
    if lower == 97 then yes = alpha
    elseif lower == 99 then yes = c < 32 or c == 127
    elseif lower == 100 then yes = digit
    elseif lower == 103 then yes = c >= 33 and c <= 126
    elseif lower == 108 then yes = c >= 97 and c <= 122
    elseif lower == 112 then yes = c >= 33 and c <= 126 and not alpha and not digit
    elseif lower == 115 then yes = c == 32 or (c >= 9 and c <= 13)
    elseif lower == 117 then yes = c >= 65 and c <= 90
    elseif lower == 119 then yes = alpha or digit
    elseif lower == 120 then yes = digit or (c >= 65 and c <= 70) or (c >= 97 and c <= 102)
    elseif lower == 122 then yes = c == 0
    else return c == p end
    if lower ~= p then return not yes end
    return yes
end

-- Return the first byte after an atom. Parse lazily, like Lua's matcher:
-- malformed suffixes are errors only when matching actually reaches them.
local function atomend(p, j)
    local c = byte(p, j)
    if c == 37 then
        if j == #p then raise("malformed pattern (ends with '%')") end
        return j + 2
    end
    if c ~= 91 then return j + 1 end
    local k = j + 1
    if byte(p, k) == 94 then k = k + 1 end
    repeat
        if k > #p then raise("malformed pattern (missing ']')") end
        if byte(p, k) == 37 and k < #p then k = k + 1 end
        k = k + 1
    until byte(p, k) == 93
    return k + 1
end

local function bracket(c, p, j, finish)
    local positive = true
    j = j + 1
    if byte(p, j) == 94 then positive = false; j = j + 1 end
    while j < finish - 1 do
        local b = byte(p, j)
        if b == 37 then
            j = j + 1
            if class(c, byte(p, j)) then return positive end
        elseif byte(p, j + 1) == 45 and j + 2 < finish - 1 then
            if b <= c and c <= byte(p, j + 2) then return positive end
            j = j + 2
        elseif b == c then return positive end
        j = j + 1
    end
    return not positive
end

local function atom(c, p, j, finish)
    if not c then return false end
    local b = byte(p, j)
    if b == 46 then return true end
    if b == 37 then return class(c, byte(p, j + 1)) end
    if b == 91 then return bracket(c, p, j, finish) end
    return b == c
end

local function match(m, i, j, depth)
    if depth == 200 then raise('pattern too complex') end
    local s, p = m.s, m.p
    while true do
        local b = byte(p, j)
        if not b then return i end
        if b == 40 then
            if #m.caps == 32 then raise('too many captures') end
            local position = byte(p, j + 1) == 41
            local cap = {i, position and -2 or -1}
            m.caps[#m.caps + 1] = cap
            local result = match(m, i, j + (position and 2 or 1), depth + 1)
            if result then return result end
            m.caps[#m.caps] = nil
            return nil
        elseif b == 41 then
            local cap
            for k = #m.caps, 1, -1 do
                if m.caps[k][2] == -1 then cap = m.caps[k]; break end
            end
            if not cap then raise('invalid pattern capture') end
            cap[2] = i - cap[1]
            local result = match(m, i, j + 1, depth + 1)
            if result then return result end
            cap[2] = -1
            return nil
        elseif b == 36 and j == #p then
            if i == #s + 1 then return i end
            return nil
        elseif b == 37 and byte(p, j + 1) == 98 then
            local open, close = byte(p, j + 2), byte(p, j + 3)
            if not close then raise("malformed pattern (missing arguments to '%b')") end
            if byte(s, i) ~= open then return nil end
            local nesting = 1
            i = i + 1
            while i <= #s do
                local c = byte(s, i)
                if c == close then nesting = nesting - 1
                elseif c == open then nesting = nesting + 1 end
                i = i + 1
                if nesting == 0 then break end
            end
            if nesting ~= 0 then return nil end
            j = j + 4
        elseif b == 37 and byte(p, j + 1) == 102 then
            if byte(p, j + 2) ~= 91 then raise("missing '[' after '%f' in pattern") end
            local finish = atomend(p, j + 2)
            local previous = i == 1 and 0 or byte(s, i - 1)
            if bracket(previous, p, j + 2, finish)
                or not bracket(byte(s, i) or 0, p, j + 2, finish) then return nil end
            j = finish
        elseif b == 37 and byte(p, j + 1) and byte(p, j + 1) >= 48 and byte(p, j + 1) <= 57 then
            local cap = m.caps[byte(p, j + 1) - 48]
            if not cap or cap[2] == -1 then raise('invalid capture index') end
            if cap[2] < 0 or #s - i + 1 < cap[2] then return nil end
            for k = 0, cap[2] - 1 do
                if byte(s, cap[1] + k) ~= byte(s, i + k) then return nil end
            end
            i, j = i + cap[2], j + 2
        else
            local finish = atomend(p, j)
            local suffix = byte(p, finish)
            local yes = atom(byte(s, i), p, j, finish)
            if suffix == 63 then
                if yes then
                    local result = match(m, i + 1, finish + 1, depth + 1)
                    if result then return result end
                end
                j = finish + 1
            elseif suffix == 42 or suffix == 43 then
                if suffix == 43 and not yes then return nil end
                local last = i
                while atom(byte(s, last), p, j, finish) do last = last + 1 end
                local minimum = i + (suffix == 43 and 1 or 0)
                for nextpos = last, minimum, -1 do
                    local result = match(m, nextpos, finish + 1, depth + 1)
                    if result then return result end
                end
                return nil
            elseif suffix == 45 then
                repeat
                    local result = match(m, i, finish + 1, depth + 1)
                    if result then return result end
                    if not atom(byte(s, i), p, j, finish) then return nil end
                    i = i + 1
                until false
            else
                if not yes then return nil end
                i, j = i + 1, finish
            end
        end
    end
end

local function capture(m, k, first, finish)
    if k == 0 or (k == 1 and #m.caps == 0) then return sub(m.s, first, finish - 1) end
    local cap = m.caps[k]
    if not cap then raise('invalid capture index') end
    if cap[2] == -1 then raise('unfinished capture') end
    if cap[2] == -2 then return cap[1] end
    return sub(m.s, cap[1], cap[1] + cap[2] - 1)
end

local function captures(m, first, finish, whole)
    local values = {}
    local n = #m.caps
    if n == 0 and whole then n = 1 end
    for k = 1, n do values[k] = capture(m, k, first, finish) end
    return unpack(values, 1, n)
end

local function search(s, p, init, plain, finding)
    s, p = text(s), text(p)
    local first = startpos(s, init)
    if first > #s + 1 then return nil end
    if finding and not plain then
        plain = true
        for k = 1, #p do
            if special[byte(p, k)] then plain = false; break end
        end
    end
    if plain then
        for i = first, #s - #p + 1 do
            local k = 1
            while k <= #p and byte(s, i + k - 1) == byte(p, k) do k = k + 1 end
            if k > #p then return i, i + #p - 1 end
        end
        return nil
    end
    local anchor = byte(p, 1) == 94
    local m = {s = s, p = p, caps = {}}
    for i = first, #s + 1 do
        m.caps = {}
        local finish = match(m, i, anchor and 2 or 1, 0)
        if finish then
            if finding then return i, finish - 1, captures(m, i, finish, false) end
            return captures(m, i, finish, true)
        end
        if anchor then break end
    end
    return nil
end

local function gmatch(s, p, init)
    s, p = text(s), text(p)
    local i, last = startpos(s, init), nil
    local m = {s = s, p = p, caps = {}}
    return function()
        while i <= #s + 1 do
            m.caps = {}
            local first = i
            local finish = match(m, i, 1, 0) -- ^ has no special meaning for gmatch.
            if finish and finish ~= last then
                i, last = finish, finish
                return captures(m, first, finish, true)
            end
            i = i + 1
        end
    end
end

local function replacement(m, repl, first, finish)
    local t = kind(repl)
    if t == 'function' or t == 'table' then
        local value
        if t == 'function' then value = repl(captures(m, first, finish, true))
        else value = repl[capture(m, 1, first, finish)] end
        if value == nil or value == false then return sub(m.s, first, finish - 1) end
        if kind(value) ~= 'string' and kind(value) ~= 'number' then raise('invalid replacement value') end
        return text(value)
    end
    local parts, j, start = {}, 1, 1
    while j <= #repl do
        if byte(repl, j) == 37 then
            parts[#parts + 1] = sub(repl, start, j - 1)
            local c = byte(repl, j + 1)
            if c == 37 then parts[#parts + 1] = '%'
            elseif c and c >= 48 and c <= 57 then parts[#parts + 1] = capture(m, c - 48, first, finish)
            else raise("invalid use of '%' in replacement string") end
            j = j + 2
            start = j
        else j = j + 1 end
    end
    parts[#parts + 1] = sub(repl, start)
    return concat(parts)
end

local function gsub(s, p, repl, maximum)
    s, p = text(s), text(p)
    local t = kind(repl)
    if t ~= 'table' and t ~= 'function' then repl = text(repl) end
    maximum = int(maximum, #s + 1)
    local m = {s = s, p = p, caps = {}}
    local anchor = byte(p, 1) == 94
    local parts, count, i, last, literal = {}, 0, 1, nil, 1
    while count < maximum do
        m.caps = {}
        local finish = match(m, i, anchor and 2 or 1, 0)
        if finish and finish ~= last then
            parts[#parts + 1] = sub(s, literal, i - 1)
            parts[#parts + 1] = replacement(m, repl, i, finish)
            count, i, last, literal = count + 1, finish, finish, finish
        elseif i <= #s then i = i + 1
        else break end
        if anchor then break end
    end
    parts[#parts + 1] = sub(s, literal)
    return concat(parts), count
end

return {
    find = function(s, p, init, plain) return search(s, p, init, plain, true) end,
    match = function(s, p, init) return search(s, p, init, false, false) end,
    gmatch = gmatch,
    gsub = gsub,
}
