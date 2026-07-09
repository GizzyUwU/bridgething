---
name: bridgething
description: Build, run, and ship a bridgething webapp for the Spotify Car Thing. Covers @bridgething/client (the SDK a webapp uses to reach the on-device daemon - now-playing, artwork, storage, settings, networking, all 18 surfaces), the loop for running the app and driving its 800x480 screen and physical controls from a browser you control, and installing the built app onto a device or packaging it as a shareable zip. Use whenever calling the daemon, running or previewing the app, pressing its controls, or pushing it to hardware.
---

# Building a bridgething webapp

A bridgething webapp is a single-page React UI that runs full-screen in the
device's chromium kiosk and reaches the on-device daemon through one typed
client. You build it against a browser on your own machine and push it to real
hardware only for final checks.

Three jobs. Read the reference file for the one in front of you:

- **Call the daemon** - now-playing, transport, artwork, storage, settings,
  networking, and the full 18-surface index: [reference/sdk.md](reference/sdk.md)
- **Run and drive the app** - dev server, screenshot the 800x480 screen, press
  the physical controls (locally, then over CDP on the device):
  [reference/develop.md](reference/develop.md)
- **Install and share** - build, push onto a connected Car Thing, package a zip
  anyone can install, and keep the device on the latest bridgething release:
  [reference/ship.md](reference/ship.md)

The device shape, the control-to-event mapping, and the hard constraints live in
CLAUDE.md, which is always loaded. This skill is the how-to on top of it.
