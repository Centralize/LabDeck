-- Counter (Lua): each press adds one; the count is saved in the key's settings.
-- LabDeck calls the global function named after each event with the message as a table.

function willAppear(ev)
  local count = ev.payload.settings.count or 0
  labdeck.setTitle(ev.context, tostring(count))
end

function keyDown(ev)
  local count = (ev.payload.settings.count or 0) + 1
  labdeck.setSettings(ev.context, { count = count })
  labdeck.setTitle(ev.context, tostring(count))
  if count % 10 == 0 then
    labdeck.showOk(ev.context)
  end
end
