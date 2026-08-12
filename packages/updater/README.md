# @bridgething/updater

CLI that brings a [bridgething](https://github.com/JoeyEamigh/bridgething) Spotify
Car Thing to a release on a channel.

It updates the **device** (daemon binary and/or full image), not a webapp. To
install or share a webapp, use [`@bridgething/client`](https://www.npmjs.com/package/@bridgething/client)
and the `push`/`share` scripts a [`create-bridgething`](https://www.npmjs.com/package/create-bridgething)
project ships.

```sh
bunx @bridgething/updater
```

Connects to a Car Thing over the daemon's network gateway (the USB-gadget link by
default), reads the discover manifest, resolves the target channel's `latest`
composite version, and applies it. If the image half of the version changed it
pushes a full image OTA, otherwise it pushes just the daemon binary, preferring a
published delta over a full artifact. A release that is yanked or deprecated is
refused.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--root <url>` | `https://ota.bridgething.com` | Manifest + artifact root. |
| `--channel <name>` | the channel the device reports | Channel to track. |
| `--host <ws-url>` | `ws://bridgething.local:8892/` | Daemon network gateway. |
| `--cache-dir <path>` | a directory under the OS tmpdir | Artifact download cache. |
| `--version <ver>` | the channel's `latest` | Composite version to install. |

Multiple devices on the network resolve to distinct mDNS names; point `--host` at
the one you mean:

```sh
bunx @bridgething/updater --host ws://bridgething-<serial>.local:8892/
```

## What it is built on

Everything below the flags - the wire protocol, transfer pacing, delta selection,
artifact downloads, and the OTA state machine - runs in the Rust delivery core,
reached through [`@bridgething/core-node`](https://www.npmjs.com/package/@bridgething/core-node).
Drive updates from your own program by using that package directly. For the same
thing in a browser, use [`@bridgething/browser`](https://www.npmjs.com/package/@bridgething/browser).

## Posture

The network gateway has no auth, matching the project posture. Treat the update
path like a debug interface: run it over the USB-CDC-ECM gadget link or a trusted
LAN, not an exposed one.
