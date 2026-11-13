# How LabDeck works

The rules and values LabDeck uses. A number such as 0x1402c9f30 after a rule is the address of the
reverse-engineered function it was taken from. Values marked **estimated** were looked for and not
found, so LabDeck chose them; the reason is given. The addresses come from two builds of the
reverse-engineered program: those in "Held keys repeat", "How keys are sent" and the Multi Action
step timings come from the newer one, so they don't line up with the rest.

## Supported devices (0x1401348f0)

Every device uses USB vendor id **0x0FD9**. Each product id has a key grid (columns × rows) and a key
picture size, set by the model's constructor.

| Device | PID | Grid | Key px | Source |
|---|---|---|---|---|
| Stream Deck (first generation) | 0x0060 | 5×3 | 72 | grid shared with 0x006d (**estimated**) |
| Stream Deck | 0x006d | 5×3 | 72 | 0x1405f00f0 |
| Stream Deck MK.2 | 0x0080 | 5×3 | 72 | 0x1405f0280 |
| Stream Deck (scissor keys) | 0x00a5 | 5×3 | 72 | 0x1405f1300 |
| Stream Deck Mini | 0x0063 | 3×2 | 80 | 0x1405f0470 |
| Stream Deck Mini | 0x0090 | 3×2 | 80 | 0x1405f06d0 (grid **estimated** same as 0x0063) |
| Stream Deck XL | 0x006c | 8×4 | 96 | 0x1405f08f0 |
| Stream Deck XL | 0x008f | 8×4 | 96 | 0x1405f0a80 |
| Stream Deck + | 0x0084 | 4×2 | 120 | 0x1405f0c10 |
| Stream Deck Neo | 0x009a | 4×2 (+2 touch) | 96 | 0x1405f1160 |
| Stream Deck Module 6 | 0x00b8 | 3×2 | 80 | as 0x1405f0470 |
| Stream Deck Module 15 | 0x00b9 | 5×3 | 72 | as 0x1405f0280 |
| Stream Deck Module 32 | 0x00ba | 8×4 | 96 | as 0x1405f0a80 |
| Stream Deck Pedal | 0x0086 | 3 keys, no screens | – | 0x1405f15e0 |

Model codes for the Module decks in saved profiles: **estimated** (no profile with them was available).

## Talking to the device

LabDeck talks to the device through the hidapi library, on its own thread.

- Key picture (0x14062dcf0): 1024-byte output reports, header
  `02 07 key last len_lo len_hi page_lo page_hi`, up to 1016 (0x3f8) bytes of JPEG each.
- Brightness (0x14062daa0): 32-byte feature report `03 08 percent`.
- Reset (0x14062da40): 32-byte feature report `03 02`, sent on connect.
- Key presses (0x14061abe0): 512-byte input reports, byte 1 = event type (0 = keys; other types are
  ignored), bytes 2..3 = payload length (at most 512), key states from byte 4. First-generation
  15-key deck (0x1406185e0): 64-byte reports, key states at bytes 1..15.
- Not used: full-panel picture `02 09 00 last page_lo page_hi len_lo len_hi` (0x14062dbb0) and the
  feature reports `03 0d` (0x14062db30), `03 05` (0x14062e190), `03 06` (0x14062e230), firmware
  `05` (0x14061ab50), serial `06` (0x14061aa40) and firmware upload `02 05` (0x14061aca0).
- JPEG quality: 90 for the XL, + and 0x006d deck, 97 for the MK.2, scissor-key deck and Neo (offset
  0xbc in each model's constructor; reading it as the quality is an inference).
- Every model's constructor also stores a size pair at 0xcc (50, 72 or 100 px) and 75 at 0x11c.
  75 equals the "High" brightness level (0x1402c1e50), so it may be the default key brightness;
  that's unconfirmed, so LabDeck's default stays at its **estimated** 70 %.
- **Estimated**: picture rotation per model (180° except the +), the first-generation deck's and
  Mini's BMP header `02 01 page 00 last key+1` and their brightness report `05 55 aa d1 01 pct`.

## Action list (0x1402221c0, 0x14021dfe0)

Groups, in order: Multi Action, Navigation, Soundboard, Stream Deck, System. Plugins in LabDeck's own
`Plugins` folder follow, sorted by name; plugins whose readable manifest lists no actions (an icon
library, a tutorial) are skipped.

Group contents, in order:
- System: Website, Hotkey Switch, Hotkey, Timed Hotkey (LabDeck's own), Open, Open Application,
  Close, Text, Multimedia, Digital Time.
- Soundboard: Play Audio, Stop Audio.
- Stream Deck: Timer, Brightness, Sleep, Action Bar, Toggle Virtual Stream Deck.
- Multi Action: Multi Action, Multi Action Switch, Random Action, Key Logic.
- Navigation: Create Folder, Switch Profile, Previous Page, Next Page, Go to Page, Page Indicator.

Action ids are `com.elgato.streamdeck.<group>.<name>`, as saved in profiles. Each action is shown
for the device's controller type
(`Controllers`, Keypad when missing): on keypad decks LabDeck hides dial-only and Neo-only actions
(Digital Time, the Neo Page Indicator) and Delay, which is only a Multi Action step. Key Logic,
Action Bar and Toggle Virtual Stream Deck are listed but not built yet. Multi Action steps exclude
the actions marked `SupportedInMultiActions: false` (folders, page actions, Hotkey Switch, Timer,
Multi Actions themselves, Key Logic, Parent Folder).

## Built-in action settings (as saved in profiles)

- Hotkey / Hotkey Switch: `Hotkeys` list of 4 entries
  `{KeyCtrl, KeyShift, KeyOption, KeyCmd, KeyModifiers, VKeyCode, QTKeyCode, NativeCode}`;
  `KeyModifiers` bits: Shift 1, Ctrl 2, Alt 4, Win 8 (0x1402bbaa0); empty slot `VKeyCode -1`,
  `QTKeyCode 33554431`.
- Text: `pastedText`, `isSendingEnter`, `isTypingMode`.
- Website: `path`, `openInBrowser`. Open: `path`. Open Application: `exec`, `args`.
- Multimedia `actionIdx` (table 0x1402ca990, 23 entries, each with its own key picture):
  0 Previous Track, 1 Play / Pause, 2 Next Track, 3 Stop, 4 Mute, 5 Volume Up, 6 Volume Down,
  7 Display Brightness Up, 8 Display Brightness Down, 9 Contrast Up, 10 Contrast Down, 11 Eject,
  12 Video Mirror, 13 Fast Forward, 14 Rewind, 15 Keyboard Brightness Up, 16 Keyboard Brightness
  Down, 17 Keyboard Backlight; the last five entries are dial-only (`actionIdx` −1).
  LabDeck sends 0–6 as the standard media keys; 7–17 are Mac keyboard keys with no standard key
  code, so a key saved with one shows its name and says it can't be sent. Each choice gets its own
  key picture (LabDeck's own line icons: previous, play/pause, next, stop, mute, volume up, volume
  down). A key with no `actionIdx` acts as 1 (Play / Pause), the value a new key is given
  (**estimated**: the settings reader 0x1407a66f0 wasn't read).
- Brightness `actionIdx` (0x14038ff80, rule 0x1402c1e50): 0 Brighter = +10 capped at 100,
  1 Darker = −10 floored at 0, 2 Maximum = 100, 3 High = 75, 4 Medium = 50, 5 Low = 25, 6 Minimum = 0.
  Key down (0x1402c1d20; newer build 0x1402c6f20) applies one step at once, then
  repeats it while held only when `actionIdx` (read by 0x1402c1800) is 0 or 1; key up (0x1402c1d70)
  stops it. The repeat timing is the keyboard's (see "Held keys repeat").
- Multi Action: runs on key **up** (0x1402c9f30). Run states: 0 idle, 1 starting, 2 running,
  3 canceling (key still down), 4 canceling (key released). Key down (0x1402c9c40): 0 → nothing yet;
  2 → 3 and the remaining steps are cancelled; 1, 3, 4 → ignored. Key up: 0 → 1 and the steps start;
  1, 2 → ignored; 3 → 4; 4 → 0. So a press while it runs cancels it and that press's release starts
  nothing; presses while it is stopping are ignored; once it has stopped, the next release starts
  it again.
- Random Action (0x1402c9f30): one 32-bit random value scaled to the entry count,
  index = `(r32 * count) >> 32`, so every entry is equally likely (`random_index`).
- Multi Action Switch (`…routine2`) runs the list for its current state, then flips.
- Multi Action steps (step runner 0x140252290, started by a single-shot timer from 0x140251fc0):
  for each step it asks the step for its two timings (down, up), presses it (key down), waits
  `down` ms (none if 0), releases it (key up), waits `up` ms (at least 1), then goes to the next
  step at once. The default timings are **100 ms and 100 ms** (0x1403bcdc0: a per-step or
  per-action override if set, else 100/100), so a Hotkey step holds its keys 100 ms and steps run
  about 200 ms apart (`MULTI_STEP_DOWN_MS`, `MULTI_STEP_UP_MS`). A second press cuts the waits
  short, but a held step is always released.
- Delay step: timings (0, `delay`) (0x14084af80), so it waits `delay` ms, at least 1 ms
  (`MULTI_STEP_MIN_WAIT_MS`).
- Timer: `duration` seconds (default 300 s, **estimated**: not found). Go to Page: `PageIndex`
  (treated as 1-based, **estimated**).
- Create Folder: `ProfileUUID` = id of the folder's page.

## Profiles on disk

`<UUID>.sdProfile/manifest.json` = `{Device{Model,UUID}, Name, Pages{Current, Default, Pages[]}, Version}`;
each page in `Profiles/<folder>/manifest.json` = `{Controllers[{Actions{"col,row": action}, Type "Keypad"}], Icon, Name}`.
Page folder name: in version 3.0 profiles the upper-case page UUID; in version 2.0 profiles the
UUID's 16 bytes in base-32 with alphabet `0123456789ABCDEFGHIJKLMNOPQRSTVW`, then `Z`. Both are
read; LabDeck saves 3.0. Key pictures are paths relative to the page folder (`Images/<name>.png`).
Plugins whose manifest is encrypted are named from the `Plugin` field saved on keys in the profiles,
else from the plugin folder's name. Title defaults: FontSize 9, OutlineThickness 2, TitleAlignment
bottom, TitleColor #ffffff.

## Window

Title bar 31 px (#20201f), device view #2d2d2d, action list 295 px wide (#292929, open groups
#222222), keys 78 px every 93 px with a 3 px #484848 ring and radius 12, empty keys #222222, page
pill #373737 with the current page #c8c8c8, category rows 50 px, action rows 40 px, splitter at
59.75 % of the height. A key held on the device is shown darker in the window while it is held.

Window buttons: LabDeck draws its own and names them in the display language (Windows:
GetUserDefaultUILanguage; Linux/macOS: LANGUAGE, LC_ALL, LC_MESSAGES, LANG). Danish "Minimer",
"Maksimer", "Luk" and English "Minimize", "Maximize", "Close" are taken from captures of Windows'
own buttons; the other languages' wording and the Restore names are **estimated** (Windows' usual
wording).

## Estimated, with reasons

- Default key brightness 70 %: not found (75 is the unconfirmed candidate above).
- Timer default 300 s and its 3 s red flash when done: not found.
- Selected-key ring colour: no capture with a selected key.
- Title size 1.5 px per point on a 72 px key: fitted to captured keys.

## Running in the background and keeping the device in sync

- Device thread (0x140595c70, scheduler 0x140593050): LabDeck's USB thread waits on its command
  channel and sends key pictures and brightness at once.
- Key reading (0x140594250, non-blocking read 0x1406154e0): every 50 ms (`KEY_POLL_MS`), draining
  every report that arrived.
- Idle sleep (fading the screens in 5 steps 50 ms apart) is not built; the Sleep action is.
- Closing the window keeps LabDeck running in the tray (setQuitOnLastWindowClosed import).
- Tray (0x1401dd4d0, 0x1401de060): tooltip "LabDeck"; menu "Configure LabDeck", "Preferences…",
  "Quit LabDeck"; a single or double click opens the window.
- Quit (0x14018bb80): "Do you want to quit LabDeck?" / "Pressing keys won't have any effect until
  you launch this app again." with Quit, Cancel and "Don't ask me again" (kept in settings.json).
- Live keys (clock, timer) are refreshed every 100 ms (`SYNC_INTERVAL_MS`, **estimated**); only
  keys whose look changed are drawn and sent.
- One copy drives the deck (LabDeck's own rule): the first LabDeck to start locks `LabDeck.lock`
  in its folder for as long as it runs (the system lets go when the process ends, even after a
  crash). A copy started while it runs opens only a window: it doesn't open the deck, start
  plugins, show a tray icon or save, and it says so in the status line; started in the tray it
  closes at once. Before this, a second copy (a double click, or a check taking a picture of the
  window) cleared and redrew the deck and read its keys, and when it closed the first copy still
  believed its own pictures were on the deck, so the deck and the window no longer matched.
- Opening the window from the tray draws every key on the deck again (and sends the brightness),
  so the deck and the window agree even if something else drew on the deck meanwhile.

## Timer end sound

- When the time is up the key shows the alarm picture (0x1402d9a80) and plays its sound
  (0x1402d9580); a chosen file is saved as `lastUserFile` in the key's settings.
- LabDeck plays `lastUserFile` if that file exists. If the chosen file is missing (checked as in
  0x1402d9460) it says so in the status line and rings its own bell instead, which also plays when
  no file is chosen: 3 strikes 0.6 s apart, 880 Hz with partials 1, 2, 2.76, 5.4, 8.93, dying away
  over about 1.4 s (**estimated**: designed here). The key flashes red for 3 s.

## Hotkey press and release

- Saved form (0x1402bbaa0): each Hotkeys entry has KeyShift = bit 0, KeyCtrl = bit 1, KeyOption =
  bit 2, KeyCmd = bit 3 of its modifier mask, plus NativeCode, QTKeyCode, VKeyCode, KeyModifiers;
  then `Coalesce`.
- Key down (0x1402bc670; newer build 0x1402c1540): picks the entry for the current state (only
  that entry: entries are 24 bytes apart), presses the modifiers, then the key, and keeps both
  held. Key up (0x1402bc620, 0x1402bc710; newer build 0x1402c15f0): releases the key first, then
  the modifiers. So the keys stay held as long as the deck key is held, and the key repeats like a
  held keyboard key (see "Held keys repeat"); the timer started at key down is that repeat, not a
  time limit. Hotkey Switch uses the entry for its state and flips after. In Multi Actions a Hotkey
  step is held for 100 ms.
- Recording a combination in the editor: the entry stores the key, not the symbol Shift makes
  (VKeyCode is a key code). Number-row keys come from the key's position (Shift+1 is always
  Shift + 1); letters, named keys and unshifted punctuation from the keyboard layout; a symbol with
  no name falls back to its position, then to the US key it sits on ("!" → 1). Playback on Windows
  sends the stored key by its scan code in the layout of the program in front, so it doesn't
  depend on which character that layout puts there.

## How keys are sent

- Down (0x14077e2e0): the modifiers go down together in one SendInput (0x1407807c0), in the order
  of their mask bits from bit 0 up: **Shift, Ctrl, Alt, Win**. Then the key goes down in a
  SendInput of its own (0x140780600). Up: the key comes up first, then the modifiers in one
  SendInput, from the top bit down: Win, Alt, Ctrl, Shift.
- Repeat (0x14077e360): down and up of the key alone with a repeat flag; an up with that flag is
  not sent (0x140780600), so a repeat is **the key down again**, modifiers untouched. That holds
  for hotkeys and media keys.
- Scan codes (0x1407818e0): MapVirtualKey; the navigation block (Page Up/Down, End, Home, arrows)
  and right Ctrl/Alt get the E0 prefix, media keys use MAPVK_VK_TO_VSC_EX with the keyboard layout.
  The reverse-engineered program sends ordinary keys as virtual-key code plus scan code, and E0
  keys (media keys included) as scan code with KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY and
  virtual-key code 0.
- LabDeck sends **every** key that has a scan code in scan-code mode (KEYEVENTF_SCANCODE, plus
  KEYEVENTF_EXTENDEDKEY for E0 keys), the way a real keyboard reports it (**estimated**: LabDeck's
  own rule). Games that read the keyboard through DirectInput or raw input go by scan codes. The
  scan code comes from the layout of the program in front (MapVirtualKeyEx, MAPVK_VK_TO_VSC_EX); a
  key with none (Pause) goes by its virtual-key code.
- Every key stays down at least `KEY_MIN_HOLD_MS` = 50 ms (**estimated**: games look at the
  keyboard once a frame, every 33 ms at 30 frames a second), even when press and release arrive
  together.
- One sender thread sends all keys in order and does the repeats of held keys, so presses don't
  wait for the window.
- The device thread presses Hotkey, Hotkey Switch and Multimedia keys itself the moment the deck
  reports them, from a table of the page on the deck that the window keeps up to date. While
  minimised or in the tray the window only looks every 100 ms, and before this a quick tap's down
  and up arrived together. Any other key press goes to the window first, and the device thread
  waits until the window has handled it, since it may change the page. The table is empty while
  the deck sleeps, so the first press only wakes it.
- Programs running as administrator: Windows drops keys sent to them by a program that isn't. The
  reverse-engineered program reaches them through a separate helper program with UI-access rights
  that it starts and talks to over a local socket (0x140599d90, 0x14059aec0, 0x140782280). LabDeck
  has no such helper (UI access needs a signed program installed under Program Files). It checks
  the program in front (its token's elevation) when a hotkey is pressed and says in the status
  line, once per program, to start LabDeck as administrator.
- Some games' anti-cheat throws away keys sent by programs while the game has focus. Windows takes
  the keys (SendInput reports them all), but they never reach even a low-level keyboard hook. No
  program that sends keys gets through that, and LabDeck doesn't try to get around it.

## Held keys repeat

Brighter/Darker, Hotkey and Hotkey Switch and Multimedia keys repeat while held, at this
computer's keyboard repeat settings, which are read each time a key goes down (SystemParametersInfo
SPI_GETKEYBOARDDELAY and SPI_GETKEYBOARDSPEED, `keyboard_repeat`):

- First repeat after **250 + 250 × delay** ms for the keyboard delay setting 0..3, i.e. 250, 500,
  750 or 1000 ms (0x1402c6d90 for brightness and media, 0x1402c1230 for hotkeys: 250 − delay ×
  −750.0 / 3.0, constants at 0x1416df900 and 0x1416df6d8). If the setting can't be read, or the
  result isn't above 0: **500 ms**.
- Then every **1000 / (speed × 27.5 / 31 + 2.5)** ms for the keyboard speed setting 0..31, i.e.
  2.5 to 30 repeats a second (400 ms down to 33 ms) (ticks 0x1402c6e10 brightness, 0x1402c12b0
  hotkey, 0x1402d1f70 media; constants 1000.0 at 0x1416df818, 27.5 at 0x1416df770, 31.0 at
  0x1416df780, 2.5 at 0x1416df6d0). Unreadable or not above 0: **100 ms**.
- Each repeat: brightness takes another step; a hotkey or media key goes down again (see "How keys
  are sent"). Key up stops the repeat and releases.
- Linux and macOS: the keyboard settings aren't read yet, so the 500 ms / 100 ms fallbacks apply.
- Before each repeat the reverse-engineered program checks that the signed-in Windows user is the
  active console user (0x140806660, WTSQuerySessionInformation) and releases the key if not.
  LabDeck doesn't make that check; the key is released when the deck key comes up.

## Timed Hotkey (LabDeck's own action)

Added at the person's request, listed in System right after Hotkey. Id
`com.labdeck.system.timedhotkey`. Settings: `Hotkeys` (one entry, same format as Hotkey), `delayMs`
(wait before, default 0), `holdMs` (keys held down, default 100, at least 50 like every key), `repeatMs` (0 = once),
`repeatCount` (0 = until pressed again). Runs on its own thread; a second press stops it and the keys
are always released. Inside a Multi Action it runs as a normal step. Each press uses the Hotkey order
(modifiers down, key down … key up, modifiers up) and the same modifier flags.

## Profile selection

- The profile picker under the device name lists every profile in LabDeck's folder, plus those in
  its `Import` folder made for a supported device, with its device and page count; the selected one
  is highlighted. Each profile keeps its own pages, current page, keys, actions and settings.
- Switching saves the profile being left first, and the selection is remembered (`last_profile` in
  settings.json) and reopened at the next start; without one, the device's default profile
  (`AppIdentifier` "*") or else the most recently saved profile opens.

## Dropping a picture on a key

- Dragging a PNG (also JPG, GIF, BMP, WebP, ICO; the list is **estimated**: no drop code was found)
  onto a key that has an action makes it the key's icon (the state's `Image`; the drawn icon is
  turned off); the key under the pointer is outlined while dragging. Windows sends no pointer moves
  during a file drag, so the system cursor position is read and mapped into the window; when it
  can't be (e.g. Wayland) the selected key is used. On an empty key it asks to put an action there
  first. The file is copied into the profile's Images folder when the profile is saved.

## Key right-click menu (0x140862d20)

- Items appear only when they apply, in this order: Copy, Paste (flag bit 1), Create Folder (bit 3),
  Move to Folder (bit 4), Create Multi Action (bit 5), Move to Multi Action (bit 6), Delete,
  Trigger Action. Other bits are for dial and Key Logic items LabDeck doesn't have.
- On an empty key **Create Folder** comes first and is shown as the default (bold), then Paste
  (when something was copied) and Create Multi Action (this order is the person's request). On a
  filled key: Copy, Paste, Move to Folder, Move to Multi Action, Delete, Trigger Action. Move to
  Folder is offered on every key except a folder's back key; Move to Multi Action only on actions a
  Multi Action may hold.
- Move to Folder: a new folder takes the key's place and the key goes inside it at the same
  position, or at the first free one when it was in the back-key corner (**estimated**). Move to
  Multi Action: a new Multi Action takes the key's place with the key's action as its first step
  (**estimated**).
- Keyboard: with a key selected and no text field being typed in, Ctrl+C copies it, Ctrl+V pastes
  the copy onto it (replacing what is there, like the menu's Paste) and Delete removes it. A copy
  is also put on the system clipboard as the key's JSON (the profile format for one key): the
  window only reports Ctrl+V when the clipboard holds something, and this way a key can be pasted
  after LabDeck restarts or into another LabDeck window. Clipboard text that isn't a key (no action
  id) pastes LabDeck's own last copy instead.
- A key that moves to another page keeps its pictures: its picture paths are resolved against the
  page it came from, and saving copies them into the new page's Images folder. Copy does the same,
  so a key pasted on another page keeps its picture too (one pasted back on its own page is saved
  by its relative path again).

## Plugins

- Plugins are the `*.sdPlugin` folders in LabDeck's own `Plugins` folder, read when LabDeck starts.
  Not started in `--screenshot` mode. Full guide: Plugins.md.
- Program and Node.js plugins are started in their folder with
  `-port <port> -pluginUUID <uuid> -registerEvent registerPlugin -info <json>`, in that order
  (0x140212790), and connect to a WebSocket on 127.0.0.1 (a free port). The first message must be
  `{"event": "registerPlugin", "uuid": …}`. `CodePath` ending in `.js`/`.mjs`/`.cjs`/`.ts`/`.mts`/
  `.cts` runs with `node` (or `LABDECK_NODE`); `.lua` runs inside LabDeck (Lua 5.4); anything else
  runs as a program. On Windows plugin programs get no console window; their output goes to
  `Logs/<uuid>.log`. Quitting LabDeck ends them.
- Event and command names are the standard plugin protocol's (keyDown, keyUp, willAppear,
  willDisappear, didReceiveSettings, didReceiveGlobalSettings, deviceDidConnect,
  deviceDidDisconnect; setTitle, setImage, setState, setSettings, getSettings, setGlobalSettings,
  getGlobalSettings, showOk, showAlert, openUrl, logMessage, switchToProfile). They aren't stored
  as plain text in the reverse-engineered code, so they couldn't be checked there (**estimated**).
- A key's context is its action id. willAppear / willDisappear follow the keys on the shown page.
  A plugin key with several states moves to the next one after keyUp unless the action sets
  `DisableAutomaticStates`. In a Multi Action a plugin step gets keyDown then keyUp at once
  (`isInMultiAction` true); `SupportedInMultiActions: false` keeps an action out.
- showOk / showAlert colour the key green / red for 1.5 s (`PLUGIN_FLASH_SECONDS`,
  **estimated**). Titles and pictures from setTitle / setImage last while LabDeck runs; settings
  and states are saved with the profile; global settings go to `PluginSettings/<uuid>.json`.
- Lua plugins: the global function named after each event is called with the message as a table,
  and `tick(seconds)` every second if defined (LabDeck's own addition); the `labdeck` table sends
  commands.

## Colours in the key editor

- Each colour (title, background, icon) has a swatch that opens the colour wheel, its code
  (`#rrggbb`), and Paste / Copy buttons. Codes are read as `#ff8800`, `#f80`, `#f808`, `ff8800`,
  `0xff8800`, `#ff8800cc` (alpha dropped), `255, 136, 0` (what the colour wheel's own copy button
  gives), `255 136 0`, `rgb(…)` / `rgba(…)`, or fractions such as `1.0, 0.5, 0.0`; a pasted code
  replaces the field.

## Website key icon

- The icon (0x140377810) is read from the page's `<link>` tags in this rel order: apple-touch-icon,
  apple-touch-icon-precomposed, icon, shortcut icon. It takes the first with a size at least the
  wanted size, else the first without a size. `//x` gets `https:`, other relative hrefs get
  `https://<host>`. Fallback (0x140377f30): `https://<host>/favicon.ico` at 16 px.
- LabDeck fetches it when Enter is pressed in the Website key's URL field (or "Get icon from
  website"); wanted size 72 px (**estimated**, the caller wasn't read). Other icons on the page are
  tried next if the pick can't be read (e.g. SVG), then favicon.ico. The key picture is 144 px: an
  icon with an opaque edge fills it, a see-through icon is centred at 62 % on black, or on a light
  tile when the icon is dark (average brightness below 0.3). Saved under LabDeck's Icons folder;
  the profile copies it into its Images folder on save.

## Long press (Timer)

- Key down starts a 500 ms timer (0x1402d0b20); a key up while it is still running stops it and
  does the short-press action (0x1402d0b40); if it fires first, the long-press action runs while the
  key is still held and the release does nothing.
- Timer keys (`LONG_PRESS_MS` = 500): short press = start, pause, resume (or start over once it has
  rung); long press = stop and reset to the key's `duration`, silencing the bell. This
  start/pause/reset split is the person's request.
