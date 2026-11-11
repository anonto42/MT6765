-- WalPad touch gestures for mpv (owner request 2026-11-11, change media-apps-capture).
--   one tap                  play / pause (after the double-tap window)
--   double tap, left third   seek -10 s
--   double tap, right third  seek +10 s
--   double tap, middle       fullscreen on / off
--   Esc (the shell's Back)   leave fullscreen, else quit
-- Taps on the on-screen controller never get here: its forced mouse areas take them first.
local mp = require 'mp'

local WINDOW = 0.32   -- seconds; just above mpv's --input-doubleclick-time (300 ms)
local timer = nil
local last_double = -1

local function now()
    return mp.get_time()
end

local function toggle_pause()
    timer = nil
    mp.commandv('cycle', 'pause')
end

local function tap()
    -- a second press that mpv also reports as a plain click right after a double click is ignored
    if now() - last_double < 0.12 then return end
    if timer then timer:kill() end
    timer = mp.add_timeout(WINDOW, toggle_pause)
end

local function double()
    if timer then
        timer:kill()
        timer = nil
    end
    last_double = now()
    local x = mp.get_mouse_pos()
    local w = mp.get_property_number('osd-width', 0)
    if w <= 0 or not x then
        mp.commandv('cycle', 'fullscreen')
        return
    end
    if x < w / 3 then
        mp.commandv('seek', '-10')
        mp.osd_message('-10 s', 0.6)
    elseif x > w * 2 / 3 then
        mp.commandv('seek', '10')
        mp.osd_message('+10 s', 0.6)
    else
        mp.commandv('cycle', 'fullscreen')
    end
end

local function back()
    if mp.get_property_bool('fullscreen') then
        mp.set_property_bool('fullscreen', false)
    else
        mp.commandv('quit')
    end
end

mp.add_key_binding(nil, 'tap', tap)
mp.add_key_binding(nil, 'double', double)
mp.add_key_binding(nil, 'back', back)
