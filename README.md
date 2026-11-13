# LabDeck

LabDeck runs an Elgato Stream Deck without Elgato's software. It's a standalone program: it
doesn't need Elgato's Stream Deck app installed and never reads that app's files. It works on
Linux, Windows and macOS. It looks and works like the original Stream Deck app:

- the device and profile picker
- the key grid with pages and folders
- the key editor under the grid
- the action list on the right

It also reads and writes the original's profile format, so your existing profiles open as they are.

Contents:

1. [What it can do](#what-it-can-do)
2. [Using LabDeck](#using-labdeck)
3. [Building it](#building-it): [Windows](#windows) · [macOS](#macos) · [Linux](#linux)
4. [Running it](#running-it)
5. [Where things are stored](#where-things-are-stored)
6. [Troubleshooting](#troubleshooting)
7. [For developers](#for-developers)

---

## What it can do

- **Devices:** Stream Deck (original, MK.2 and the newer 15-key models), Mini, XL, +, Neo, the
  Module 6/15/32 and the Pedal. LabDeck finds the device on its own and draws every key's picture
  on it. Key presses are read every 50 ms, like the original.
- **Profiles and pages:** any number of profiles, each with its own pages, folders, keys and key
  settings. Switch between them with the picker under the device name. Profiles exported from the
  original app (`.streamDeckProfile`) can be imported. On a first start with no profiles at all,
  LabDeck makes a one-page profile called **Version** with four Text keys in the middle spelling
  LAB / DECK over 0 / 4.
- **Built-in actions:**

  | Group | Actions |
  |---|---|
  | Multi Action | Multi Action, Multi Action Switch, Random Action |
  | Navigation | Create Folder, Switch Profile, Previous/Next Page, Go to Page, Page Indicator |
  | Soundboard | Play Audio, Stop Audio |
  | Stream Deck | Timer (with a bell when the time is up; hold the key to reset), Brightness, Sleep |
  | System | Website (gets the site's icon automatically), Hotkey, Hotkey Switch, Timed Hotkey, Open, Open Application, Close, Text, Multimedia |

  Key Logic, Action Bar and Toggle Virtual Stream Deck are listed like in the original but are
  not available yet.
- **Plugins:** `*.sdPlugin` folders put in LabDeck's own `Plugins` folder show up in the list and
  run: program plugins (for example written in Rust), JavaScript and TypeScript plugins (with
  Node.js) and Lua plugins (built in). How to write one, with an example in each language, is in
  [Plugins.md](Plugins.md). Plugins whose manifest is encrypted can't run, but their keys are kept.
- **Runs in the background:** closing the window keeps LabDeck running in the system tray, and the
  Stream Deck keeps working. Use the tray icon to open the window again or to quit.

---

## Using LabDeck

- **Put an action on a key:** drag it from the list on the right onto a key, or double-click it in
  the list. Right-click an empty key for **Create Folder** (the default), Paste or Create Multi
  Action.
- **Change a key:** click it. The editor underneath shows its title, picture, colours and the
  action's settings. Right-click a key for Copy, Paste, Move to Folder (puts the key into a new
  folder in its place), Move to Multi Action (makes it the first step of a new Multi Action),
  Delete and Trigger Action. With a key selected, **Ctrl+C** copies it and **Ctrl+V** pastes the
  copied key onto it (in a text field they copy and paste text as usual); **Delete** removes it.
- **Colours:** click a colour's swatch for the colour wheel. Next to it is the colour's code: type
  one in, paste one with **Ctrl+V** or the **Paste** button, or copy it with **Copy**. Codes like
  `#ff8800`, `#f80`, `ff8800`, `rgb(255, 136, 0)` and `255, 136, 0` (what the colour wheel's own
  copy button gives) all work.
- **Give a key a picture:** drag a PNG (or JPG, GIF, BMP, WebP, ICO) file onto it. For a Website
  key, type the address and press **Enter**, and the site's icon is fetched for you.
- **Pages:** use the page bar under the grid. The **+** adds a page; right-click a page to delete
  it.
- **Profiles:** click the profile name under the device name. From there you can switch, rename,
  create, duplicate, import and delete profiles.
- **Brightness:** click the device name, or the gear icon.

---

## Building it

LabDeck is written in Rust. You need the Rust toolchain, **version 1.85 or newer** (the project
uses the 2024 edition), on every platform.

1. Install Rust from <https://rustup.rs> (this gives you `cargo`).
2. Install the few system parts for your platform (see below).
3. Build it, inside the `app` folder:

   ```sh
   cargo build --release
   ```

   The first build downloads the libraries LabDeck uses, so it needs internet access and takes a
   few minutes. Later builds are much faster.

The program ends up in `target/release/`: `labdeck.exe` on Windows, `labdeck` on macOS and Linux.
For a quicker build while testing, `cargo build` puts a slower debug build in `target/debug/`.

### Windows

1. Install **Rust** with `rustup-init.exe` from <https://rustup.rs> and pick the default
   (MSVC) toolchain.
2. It asks for the **Visual Studio C++ Build Tools**. Install them with the "Desktop development
   with C++" workload (the free "Build Tools for Visual Studio" is enough).
3. Open a new terminal (PowerShell or Command Prompt), go to the `app` folder and run:

   ```powershell
   cargo build --release
   .\target\release\labdeck.exe
   ```

No extra drivers are needed: Windows gives programs access to the Stream Deck by itself. **Close
Elgato's Stream Deck app first**, because two programs can't drive the device at the same time.

The release build is a window-only program, so it doesn't open a console window. Use the debug
build (`cargo build`) if you want to see printed messages.

### macOS

1. Install Apple's command line tools:

   ```sh
   xcode-select --install
   ```

2. Install Rust:

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

3. Build and run, inside the `app` folder:

   ```sh
   cargo build --release
   ./target/release/labdeck
   ```

macOS asks for two permissions. Allow them in **System Settings → Privacy & Security**, for your
terminal or for LabDeck:

- **Accessibility** lets Hotkey, Text and Multimedia keys type for you.
- **Input Monitoring** may be needed to read the Stream Deck's buttons.

Quit Elgato's Stream Deck app before starting LabDeck.

### Linux

Build LabDeck on the Linux computer itself: a build made on Windows only runs on Windows. Copy the
`app` folder over (the `target` folder can be left behind), then:

1. Install a C compiler, `pkg-config` and the development files for **udev** (USB device access)
   and **ALSA** (sound):

   | Distribution | Command |
   |---|---|
   | Debian, Ubuntu, Mint, Pop!_OS | `sudo apt install build-essential pkg-config libudev-dev libasound2-dev` |
   | Fedora | `sudo dnf install gcc pkgconf-pkg-config systemd-devel alsa-lib-devel` |
   | Arch, Manjaro | `sudo pacman -S --needed base-devel alsa-lib` (udev comes with systemd) |
   | openSUSE | `sudo zypper install gcc pkgconf systemd-devel alsa-devel` |

2. Install Rust:

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

3. Build, inside the `app` folder:

   ```sh
   cargo build --release
   ```

4. **Let your user reach the Stream Deck** (needed once). Copy the rule file that comes with
   LabDeck and reload udev:

   ```sh
   sudo cp linux/70-streamdeck.rules /etc/udev/rules.d/
   sudo udevadm control --reload-rules && sudo udevadm trigger
   ```

   Then unplug the Stream Deck and plug it back in.

5. Run it:

   ```sh
   ./target/release/labdeck
   ```

Desktop notes:

- **Window:** LabDeck runs on X11 and Wayland. The graphics libraries it needs (OpenGL, xkbcommon,
  Wayland/X11) are part of every normal desktop install.
- **Tray icon:** it uses the standard StatusNotifier tray, which KDE, Xfce, Cinnamon and most other
  desktops show. On GNOME, install the "AppIndicator and KStatusNotifierItem Support" extension.
- **Keystrokes:** Hotkey, Text and Multimedia keys type through X11. On a pure Wayland session,
  apps that don't run under XWayland may not receive them.
- **Dropping files on keys:** where the pointer position can't be read (Wayland), a dropped
  picture goes onto the selected key.

To install it for your user only:

```sh
install -Dm755 target/release/labdeck ~/.local/bin/labdeck
```

---

## Running it

```text
labdeck                     open the window
labdeck --background        start in the tray without a window (for autostart)
labdeck --help              list the options
```

These three exist for checks and testing:

```text
labdeck --screenshot out.png   draw about 120 frames, save the window as a PNG, exit
labdeck --favicon example.com  fetch a site's icon, make a key picture, print where it went
labdeck --icons icons.png      draw every built-in icon on one sheet, save it as a PNG, exit
```

To start with your computer:

- **Windows:** press Win+R, type `shell:startup`, and put a shortcut to `labdeck.exe --background`
  in that folder.
- **macOS:** System Settings → General → Login Items → add LabDeck.
- **Linux:** create `~/.config/autostart/labdeck.desktop` with:

  ```ini
  [Desktop Entry]
  Type=Application
  Name=LabDeck
  Exec=/home/YOU/.local/bin/labdeck --background
  ```

---

## Where things are stored

| | Windows | macOS and Linux |
|---|---|---|
| LabDeck's folder | `%APPDATA%\LabDeck` | `~/.local/share/LabDeck` (or `$XDG_DATA_HOME/LabDeck`) |

Set the environment variable `LABDECK_HOME` to use another folder. Inside LabDeck's folder:

- `ProfilesV3/` holds your profiles, in the original's format.
- `Plugins/` is where to put `*.sdPlugin` folders; they're started with LabDeck (see [Plugins.md](Plugins.md)).
- `Logs/` holds each plugin's log, `PluginSettings/` their global settings and `PluginImages/`
  the pictures they set on keys.
- `Icons/` holds pictures made from websites.
- `Import/ProfilesV3/` is a place for a folder of profiles in the same format, for example copied
  from another computer. It is read (never changed) and its profiles are listed; an edited one is
  saved as LabDeck's own copy.
- `settings.json` holds the last selected profile and the quit question's "Don't ask me again".

LabDeck keeps everything in its own folder and never reads Elgato's Stream Deck app or its
folders, so it works the same whether that app is installed or not. If it is installed, quit it
while LabDeck runs, because two programs can't drive the device at the same time.

To import an exported profile, open the profile picker, choose **Import profile…** and enter the
path of the `.streamDeckProfile` file.

---

## Troubleshooting

| Problem | What to do |
|---|---|
| "Found a Stream Deck but could not open it" | Linux: install the udev rule (step 4) and replug. All systems: quit Elgato's Stream Deck app. |
| Keys stay blank | Replug the device; check that no other Stream Deck program is running. |
| The deck shows something different from the window | Open LabDeck from the tray: that draws every key again. Only one LabDeck drives the deck; a second copy says "LabDeck is already running" and only shows a window. Other Stream Deck programs must be closed, since they draw on the deck too. |
| Hotkeys don't type | macOS: allow Accessibility. Linux Wayland: the target app may need to run under XWayland. |
| A hotkey does nothing in a game (Windows) | LabDeck sends keys by hardware scan code and holds each one at least 50 ms, so games see them. If the status line says the game "runs as administrator", start LabDeck as administrator too (Windows blocks keys from normal programs to those). Games whose anti-cheat ignores keys sent by programs can't be reached by LabDeck or any similar tool. Fullscreen games: keep LabDeck in the tray; it doesn't need the window. |
| No tray icon on GNOME | Install the AppIndicator extension, or run without `--background` and keep the window open. |
| No sound from Play Audio or the Timer bell | Linux: make sure ALSA (or PipeWire/PulseAudio's ALSA layer) works; check the sound output. |
| Website icon not found | The site may block downloads or only offer an SVG icon; drop a PNG on the key instead. |
| Build error about `libudev` or `alsa` (Linux) | Install the packages in Linux step 1. |
| Build error about `link.exe` (Windows) | Install the Visual Studio C++ Build Tools (Windows step 2). |

---

## For developers

| File | What it does |
|---|---|
| `src/main.rs` | command-line options, window setup |
| `src/app.rs` | the app's state and behaviour: profiles, pages, folders, device link, timers, saving |
| `src/view.rs` | drawing the window: title bar, device view, key grid, pages, action list, menus |
| `src/inspector.rs` | the key editor under the grid |
| `src/actions.rs` | what each built-in action does |
| `src/catalog.rs` | the action list (standard groups and installed plugins) |
| `src/devices.rs` | Stream Deck models: product ids, key grids, picture sizes |
| `src/hid.rs` | USB communication with the device (its own thread); presses the page's hotkeys at once |
| `src/keys.rs` | sending key commands to other programs (scan codes, shortest hold, repeats), on its own thread |
| `src/keyimage.rs`, `src/icons.rs` | drawing key pictures and the built-in line icons |
| `src/profile.rs` | reading and writing profiles in the original's format |
| `src/favicon.rs` | fetching a website's icon |
| `src/tray.rs` | the system tray icon and menu |
| `src/plugins.rs` | running plugins: starting them, their WebSocket, Lua, messages |
| `Plugins.md`, `plugin-examples/` | how to write plugins, and a Counter example in Rust, JavaScript, TypeScript and Lua |
| `MECHANICS.md`, `mechanics.json` | how the original works (with function addresses) and the values used |
| `linux/70-streamdeck.rules` | udev rule for Linux device access |

`cargo build` makes a debug build; `cargo build --release` makes an optimised one. There are no
build scripts and no generated files: the Rust sources and `Cargo.toml` are everything.
