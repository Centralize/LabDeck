#!/bin/sh
# LabDeck setup for Linux, in one step. In a terminal, inside the app folder (the one with
# Cargo.toml in it), type:
#
#     sh linux/install.sh
#
# It installs what building needs (it asks for your password for that), installs Rust if it's
# missing or too old, builds LabDeck, puts it in ~/.local/bin, lets your user reach the Stream Deck
# and starts LabDeck. From then on LabDeck is in your app menu: you don't need a terminal for it
# again. Run it again after getting a newer LabDeck, to update it.
#
#     sh linux/install.sh --uninstall
#
# takes LabDeck out of ~/.local/bin, the app menu and autostart again (your profiles stay).

set -eu

say() { printf '\n\033[1;35m==>\033[0m %s\n' "$*"; }
fail() { printf '\n\033[1;31mLabDeck setup stopped:\033[0m %s\n' "$*" >&2; exit 1; }

if [ "$(id -u)" = 0 ]; then
    fail "run it as yourself, without sudo (it asks for your password when it needs it)."
fi

bin="$HOME/.local/bin/labdeck"
menu="${XDG_DATA_HOME:-$HOME/.local/share}/applications/labdeck.desktop"
auto="${XDG_CONFIG_HOME:-$HOME/.config}/autostart/labdeck.desktop"

if [ "${1:-}" = "--uninstall" ]; then
    say "Taking LabDeck out of ~/.local/bin, the app menu and autostart"
    rm -f "$bin" "$menu" "$auto"
    printf '\nDone. Your profiles and settings are still in %s.\n' "${LABDECK_HOME:-${XDG_DATA_HOME:-$HOME/.local/share}/LabDeck}"
    printf 'The Stream Deck access rule stays in /etc/udev/rules.d/70-streamdeck.rules.\n'
    exit 0
fi

here=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
[ -f "$here/Cargo.toml" ] || fail "can't find LabDeck's Cargo.toml next to the linux folder (looked in $here)."
cd "$here"

# 1. What building needs.
say "Installing what building LabDeck needs (a C compiler, pkg-config, udev and ALSA files)"
if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y build-essential pkg-config libudev-dev libasound2-dev curl
elif command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y gcc pkgconf-pkg-config systemd-devel alsa-lib-devel curl
elif command -v pacman >/dev/null 2>&1; then
    sudo pacman -S --needed --noconfirm base-devel alsa-lib curl
elif command -v zypper >/dev/null 2>&1; then
    sudo zypper --non-interactive install gcc pkgconf systemd-devel alsa-devel curl
else
    say "Unknown package manager: install a C compiler, pkg-config and the udev and ALSA development files yourself (README.md lists them)."
fi

# 2. Rust 1.85 or newer.
if [ -f "$HOME/.cargo/env" ]; then . "$HOME/.cargo/env"; fi
rust_ok=0
if command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
    v=$(rustc --version 2>/dev/null | cut -d' ' -f2)
    major=$(printf '%s' "$v" | cut -d. -f1)
    minor=$(printf '%s' "$v" | cut -d. -f2)
    case "$major$minor" in *[!0-9]*|'') major=0; minor=0 ;; esac
    if [ "$major" -gt 1 ] || { [ "$major" -eq 1 ] && [ "$minor" -ge 85 ]; }; then rust_ok=1; fi
fi
if [ "$rust_ok" = 0 ]; then
    if command -v rustup >/dev/null 2>&1; then
        say "Updating Rust"
        rustup update stable
        rustup default stable
    else
        say "Installing Rust (from rustup.rs)"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        . "$HOME/.cargo/env"
    fi
fi

# 3. Build.
say "Building LabDeck (the first time downloads its libraries and takes a few minutes)"
cargo build --release

# 4. Put it in ~/.local/bin (a LabDeck that is running keeps going; the new one is used next start).
say "Putting LabDeck in ~/.local/bin"
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/labdeck "$bin"

# 5. Let your user reach the Stream Deck.
say "Letting your user reach the Stream Deck (udev rule)"
sudo install -m 644 linux/70-streamdeck.rules /etc/udev/rules.d/70-streamdeck.rules
sudo udevadm control --reload-rules
sudo udevadm trigger || true

# 6. Start it, in a session of its own so this terminal can be closed. At start LabDeck adds
#    itself to the app menu.
say "Starting LabDeck"
if command -v setsid >/dev/null 2>&1; then
    setsid "$bin" --foreground </dev/null >/dev/null 2>&1 &
else
    nohup "$bin" --foreground </dev/null >/dev/null 2>&1 &
fi

printf '\n\033[1;32mDone.\033[0m LabDeck is in your app menu now (search for "LabDeck"; you can pin it to your dock).\n'
printf 'You can close this terminal.\n\n'
printf -- '- If the Stream Deck was already plugged in, unplug it and plug it back in once.\n'
printf -- '- If LabDeck was already running, quit it from its tray icon and start it again from the menu.\n'
printf -- '- To start LabDeck when you log in, click the device name in LabDeck and tick "Start LabDeck when I log in".\n'
