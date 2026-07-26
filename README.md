# bridgething

bridgething is the bridge layer that lets the thing remain itself while
opening up to anything you build for it. It runs on a fully custom Linux
distro designed specifically for the Car Thing and replaces Spotify's
stock `qt-superbird-app` with a Rust daemon, a kiosk web runtime, and a
phone-side gateway, so the Car Thing keeps being a Car Thing on your
own terms.

![the launcher](https://bridgething.com/screenshots/device-launcher.png)

<p>
  <img src="https://bridgething.com/screenshots/device-spotify.png" width="405" alt="stock spotify ui">
  <img src="https://bridgething.com/screenshots/device-calendar.png" width="405" alt="calendar">
  <img src="https://bridgething.com/screenshots/device-weather.png" width="405" alt="weather">
  <img src="https://bridgething.com/screenshots/device-home-assistant.png" width="405" alt="home assistant">
</p>

## Install

Install from [bridgething.com](https://bridgething.com) over USB. Updates
after that come over the air through your phone, which is also where
pairing, settings, and new apps live.

<p>
  <img src="https://bridgething.com/screenshots/companion-home.png" width="240" alt="companion home">
  <img src="https://bridgething.com/screenshots/companion-update.png" width="240" alt="update available">
  <img src="https://bridgething.com/screenshots/companion-settings.png" width="240" alt="companion settings">
</p>

## What's in here

| path                                       | what it is                                                        |
| ------------------------------------------ | ----------------------------------------------------------------- |
| `crates/lib`                               | `libbridgething` - the wire-protocol crate. DTOs, codec, framing. |
| `crates/core`                              | `bridgething` - the daemon                                        |
| `crates/client-rs`                         | Rust client of `libbridgething`.                                  |
| `crates/mfi`, `crates/mfi-proxy`           | iAP2 / MFi link layer                                             |
| `crates/swupdate-sys`                      | FFI to libswupdate for in-band system OTA                         |
| `packages/gateway`, `packages/companion`   | Phone-side gateway (TS + native)                                  |
| `packages/client-ts`, `packages/adapter-*` | Generated SDKs                                                    |
| `packages/webapps/builtin`                 | Webapps delivered with the daemon (hub, browser)                  |
| `packages/webapps/catalog`                 | Webapps published to the app catalog                              |
| `packages/create-bridgething`              | `bun create bridgething`                                          |
| `mobile/`                                  | Phone-side app                                                    |
| `docs/protocol.md`                         | Wire-protocol reference                                           |

## Dataflow

```text
     webapp                       daemon                  gateway
┌─────────────────┐    ws    ┌──────────────┐   bt    ┌──────────────┐
│  hub / stock /  │◄────────►│  bridgething │◄───────►│   phone +    │
│  your own app   │ ws,asset │              │ msgpack │   companion  │
└─────────────────┘          └──────────────┘         └──────────────┘
    device kiosk                the thing               phone-side
```

The webapp talks to the daemon over a local WebSocket on `127.0.0.1:8891`.
The daemon talks to the phone-side gateway over Bluetooth RFCOMM, with
an iAP2 link layer for iOS. The gateway sources playback, streams audio
out, and proxies arbitrary HTTP/WS via the `Tunnel` surface. Everything
that crosses a boundary is typed in `libbridgething`; SDKs in TS,
Swift, and Kotlin are generated from those types.

## Build a webapp

```bash
bun create bridgething
```

Creates a Vite + React + Tailwind project wired to the bridgething
client. The manifest schema is in `crates/lib/src/shared/webapp.rs`;
permissions, config primitives, and the KV namespace are documented in
`crates/core/src/state/webapps.rs` and `docs/protocol.md`.

## On-device dev

The image, OTA pipeline, and BSP live in
[`yocto-superbird`](https://github.com/JoeyEamigh/yocto-superbird).
Webapps push with `bun run push` from each webapp package.
`bun create bridgething` comes with scripts for pushing to the device.

For host-side dev, the daemon expects `chromium --remote-debugging-port=9223`
running and a Bluetooth adapter with class `0x7c0000`
(`sudo hciconfig hci0 class 0x7c0000`).
