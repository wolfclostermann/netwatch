# netwatch

A menu bar (macOS) and system tray (Windows) indicator for internet connectivity.

Every 10 s (3 s while down) it fetches Apple's captive-portal probe,
`http://captive.apple.com/hotspot-detect.html`:

| Probe result                 | Icon                         |
|------------------------------|------------------------------|
| `Success` page               | static globe, matches menu bar / taskbar theme |
| any other response           | **captive portal**: throbbing amber globe, and the login window opens |
| no response / timeout        | **offline**: throbbing red globe |

When it first detects a captive portal, netwatch opens a login window:

- **macOS**: the Captive Network Assistant. If that quits right away, which
  happens when macOS hasn't flagged the network as captive itself, netwatch
  opens the probe URL in your default browser instead, and the portal
  redirects it to its login page.
- **Windows**: `http://www.msftconnecttest.com/redirect` (the page Windows
  itself opens for captive portals) in your default browser.

You can also open it from the menu with **Open Login Window…**.

## Build & run

```sh
cargo build --release
./target/release/netwatch
```

Set `NETWATCH_PROBE_URL` to point the probe elsewhere for testing, e.g.
`http://127.0.0.1:9/` (offline) or `http://example.com/` (captive).

On Windows, release builds run without a console window, so status changes
aren't logged anywhere; use a debug build (`cargo run`) to see them.

## Start at login

### macOS

```sh
cargo install --path .
netwatch install     # registers netwatch to start at login and starts it now
netwatch uninstall   # stops it and unregisters it
```

After rebuilding, run `cargo install --path .` again and then `netwatch install`
to restart it on the new binary.

`install` writes `~/Library/LaunchAgents/local.netwatch.plist`. launchd restarts
netwatch if it crashes, but leaves it stopped after **Quit** until the next
login. Errors are logged to `~/Library/Logs/netwatch.log`.

### Windows

```powershell
.\install.ps1              # build, install and start (replacing any running copy)
.\install.ps1 -Uninstall   # stop it, unregister it and remove the installed copy
```

`install` copies the binary to `%LOCALAPPDATA%\Programs\netwatch\netwatch.exe`,
so rebuilding or `cargo clean` doesn't affect the installed copy, and adds a
`netwatch` value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
pointing at it. Rerun `.\install.ps1` after changes to replace the running copy.

Unlike launchd, Windows won't restart netwatch if it crashes. Windows may hide
new tray icons in the overflow menu; drag it onto the taskbar, or turn it on
under **Settings → Personalization → Taskbar → Other system tray icons**.

## Notes

- On MacBooks with a notch, status items that don't fit next to the active app's
  menus are hidden behind the notch. If you can't see the icon, ⌘-drag other
  menu bar icons to make room; netwatch remembers its position.
