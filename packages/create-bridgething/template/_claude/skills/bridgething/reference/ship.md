# Installing and sharing the app

Two outputs: install onto your own connected device, and a zip anyone can install
through the bridgething companion app.

## Install onto a connected Car Thing

```bash
bun run push            # targets bridgething.local (device over USB)
bun run push <address>  # or an explicit host / IP
```

`push` builds `dist/` itself, rsyncs it to the device, then tells the daemon to
switch the kiosk to your app - one command, no separate build step. The device
must be reachable: connected over USB it answers to `bridgething.local`;
otherwise pass its address. If multiple devices are on the network, set
`SUPERBIRD_HOST=bridgething-<serial>.local`.

Flags: `--skip-build` reuses the existing `dist/`; `--no-switch` copies without
activating. Iterating on hardware, rerun `bun run push` after each change. For
the fast edit loop, drive a local browser instead (see the develop reference) and
save the device for final checks.

If `manifest.json` declares `role: launcher` or an `overlay`, `push` also claims
the matching device slot, so the bundle is live rather than merely installed. A
launcher is switched to; an overlay is not, since switching away from the app it
draws over defeats the point. `--no-slot` pushes without claiming, and
`--release` gives the slots back to the built-in hub and overlay - the recovery
path when a build wedges the screen.

## Make a shareable zip

```bash
bun run build   # share does NOT build; produce dist/ first
bun run share   # writes <name>-<version>.zip from dist/
```

Send that zip to anyone with a bridgething-flashed Car Thing. They install it
from the bridgething companion app (no rebuild, no dev tools). The zip is just
your built bundle with `manifest.json` at its root. `bun run build` also emits
`dist/settings.html` (the companion-side settings page); it ships in the same
bundle when `manifest.json` declares `settings`.

Bump `version` in `public/manifest.json` before sharing an update, so people can
tell builds apart. Never touch `manifest.json`'s `id` - it is the webapp's
identity and the install path on the device.

## Keep the device on the latest bridgething

```bash
bun run update   # updates the connected Car Thing's daemon + image
```

`update` runs `@bridgething/updater`: it connects over the same gateway `push`
uses, reads the release manifest, and brings the device to the latest `stable`
release.

Multiple devices? Target one explicitly:

```bash
bun run update -- --host ws://bridgething-<serial>.local:8892/
```

Other flags: `-- --channel <name>` to track a non-stable channel,
`-- --daemon-only` to skip the image OTA.
