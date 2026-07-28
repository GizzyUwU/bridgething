
## This project is a launcher

`manifest.json` declares `"role": "launcher"`, which means two things:

- The daemon hides this bundle from `client.webapp.list`, so a launcher never
  lists itself in its own grid.
- The bundle is eligible for the device's **launcher slot**. Holding that slot
  makes this the home screen: the boot default, the target of the Mode-button
  gesture, and what the back button returns to.

`bun run push` claims the slot and switches to it, so a push puts you straight
on the home screen. `bun run push --release` hands it back, and the companion
phone app can do the same. The built-in hub stays installed and can never be
uninstalled, so releasing the slot always gets the stock home screen back. That
is the recovery path if a launcher you are building wedges the device.

### What a home screen is expected to do

Nothing is enforced. The daemon does not require a launcher to implement any
particular surface; if yours does less than the built-in hub, that is a choice,
not an error, and the user can switch back. The built-in hub covers:

- the app grid (`client.webapp.list` / `.icon` / `.activate` / `.current`, plus
  the `onWebappInstalled` / `onWebappUninstalled` events so the grid stays live)
- bluetooth settings: bonds, adapter alias, discoverable
- display brightness
- system info and health
- power: restart, shut down, factory reset
- OTA progress (`client.system.onOtaProgress` / `.onOtaError`)

The starter here implements only the grid. Read the built-in hub's source if you
want a reference for the rest; everything it uses is on the public client SDK.

### Onboarding is not your problem

The first-boot pairing wizard lives in the built-in hub. A phone must already be
paired before anyone can install your launcher, so by the time this runs, the
device is set up. A factory reset clears the launcher slot along with everything
else, which puts the built-in hub and its wizard back.
