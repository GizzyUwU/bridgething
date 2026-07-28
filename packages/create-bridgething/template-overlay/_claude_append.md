
## This project is a system overlay

`manifest.json` declares `"overlay": "overlay.js"`. When this bundle holds the
device's **overlay slot**, the daemon injects that file into every webapp's
document as it loads. The page in `src/` is incidental: it only renders if
someone launches this bundle from the hub.

`bun run push` claims the slot for you and reloads the kiosk, so the overlay
appears over whatever app is showing. It deliberately does **not** switch to
this bundle's own page - switching away from the app you are testing against is
the opposite of what you want.

`bun run push --release` hands the slot back to the daemon's built-in overlay.
That is the recovery path when a build of yours misbehaves; the companion phone
app can do the same thing.

### The contract

The daemon prepends one global before your bundle:

```js
window.__bridgethingOverlay = { origin: 'http://127.0.0.1:8891', surfaces: {...} };
```

- **`surfaces`** is the active webapp's declared profile: `notifications`,
  `call`, `pairing`, `connection`, `volume`. An app that draws its own volume
  indicator declares `volume: false`. **Honor these.** The daemon cannot enforce
  it, and ignoring them means the user sees two of everything on apps that draw
  their own. If every surface is off, the daemon injects nothing at all.
- **`origin`** is the kiosk origin. Check `location.origin` against it before
  mounting so the bundle never runs in a page the daemon did not serve.

Everything else comes from `@bridgething/client` over the local websocket, the
same SDK a normal webapp uses.

### Constraints that are not style

`overlay.js` must be **one self-contained file** under 512 KiB. It is injected as
a script string into another app's document, so it cannot import a module graph,
fetch assets, or reach a bundler at runtime. `vite.overlay.config.ts` builds it
as a single inlined iife; keep it that way.

That is a constraint on the *output*, not on how you write it. Style
`overlay/main.tsx` with tailwind classes exactly like the rest of the project.
`overlay/style.css` is a real stylesheet; it is imported with vite's `?inline`
so the compiled css lands in the bundle as a string and gets mounted into the
shadow root. Add `@theme` tokens or plain rules there. Because the shadow root
is closed, tailwind is told not to scan the project (`source(none)`) and only
generates what `overlay/main.tsx` uses - if you split the overlay across more
files, add each one with `@source`.

The starter encodes four things you should not drop:

1. **Origin guard** before mounting.
2. **`__bridgethingOverlayMounted`** guard, so a double injection is a no-op.
3. **Closed shadow root**, so your styles and the host app's cannot reach each
   other.
4. **Escape-only, capture-phase key handling**, and only while something is
   showing. Your overlay sits on top of every app on the device; swallowing keys
   broadly breaks all of them.

You are running inside someone else's page. A crash, a fullscreen paint, or a
greedy key handler takes down every webapp, not just this one.
