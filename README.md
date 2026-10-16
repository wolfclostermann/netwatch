# netwatch

A macOS menu bar indicator for internet connectivity.

Every 10 s (3 s while down) it fetches Apple's captive-portal probe,
`http://captive.apple.com/hotspot-detect.html`:

| Probe result                 | Icon                         |
|------------------------------|------------------------------|
| `Success` page               | static globe, matches menu bar theme |
| any other response           | **captive portal**: throbbing amber globe, and the login window opens |
| no response / timeout        | **offline**: throbbing red globe |

When it first detects a captive portal, netwatch opens macOS's
Captive Network Assistant. If that quits right away, which happens when macOS
hasn't flagged the network as captive itself, netwatch opens the probe URL in your
default browser instead, and the portal redirects it to its login page.
You can also open it from the menu with **Open Login Window…**.

## Build & run

```sh
cargo build --release
./target/release/netwatch
```

Set `NETWATCH_PROBE_URL` to point the probe elsewhere for testing, e.g.
`http://127.0.0.1:9/` (offline) or `http://example.com/` (captive).

## Notes

- On MacBooks with a notch, status items that don't fit next to the active app's
  menus are hidden behind the notch. If you can't see the icon, ⌘-drag other
  menu bar icons to make room; netwatch remembers its position.
