# Running and driving the app

The device is an 800x480 landscape screen with five physical controls plus touch.
To develop effectively you (1) run the app, (2) see it at the real size, and
(3) press the buttons. The fast loop drives a browser you control on your own
machine; you only need the physical Car Thing for final validation.

## Contents

- Start the dev server
- Drive it with Playwright (recommended for agents)
- Alternative: your own browser + DevTools
- Drive the real device over CDP
- Input reference

## Start the dev server

```bash
bun run dev
```

Vite serves at `http://localhost:5173/` with hot reload.

## Drive it with Playwright (recommended for agents)

Playwright is **not** installed by default (a human clicking around does not need
it). Add it only when you want to see and drive the app programmatically:

```bash
bun add -d playwright
bunx playwright install chromium
```

Then a short script gives you a locked 800x480 window you can screenshot and send
input to:

```ts
import { chromium } from 'playwright';

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 800, height: 480 } });
await page.goto('http://localhost:5173/');

await page.screenshot({ path: 'screen.png' }); // look at the result

// physical controls -> the exact events the device sends:
await page.keyboard.press('1');       // preset 1  (also '2' '3' '4')
await page.keyboard.press('m');       // Mode button
await page.keyboard.press('Escape');  // Back button
await page.mouse.wheel(120, 0);       // rotary wheel: HORIZONTAL scroll (deltaX)
// touch: page.tap('selector')  or  page.mouse.click(x, y)
```

Loop: edit code -> vite hot-reloads -> screenshot -> adjust. Keep the viewport at
exactly `{ width: 800, height: 480 }` so what you see matches the device.

## Alternative: your own browser + DevTools

Open `http://localhost:5173/` in your normal browser, then in DevTools' device
toolbar set a custom device of **800 x 480**. You get real DevTools plus the same
keys (type `1`-`4`, `m`, `Escape`; shift+wheel for the rotary's horizontal
scroll). This is the recommended manual path.

## Drive the real device over CDP

To see and drive the app on real hardware, talk CDP to the device's own chromium.
CDP input goes straight into the page (never through the physical buttons), so the
launcher gesture and on-device browser-nav side effects never interfere.

Install the app onto the device first so the kiosk is showing it (build + push).
CDP is served at `bridgething.local:9222`, reachable directly over USB:

```bash
# device answers at bridgething.local over USB; root has no password
curl -s http://bridgething.local:9222/json/version >/dev/null && echo cdp-up
```

Use the `webSocketDebuggerUrl` exactly as returned. It comes back as an IP rather
than the hostname you asked for, and it is already correct - do not rewrite it.

**Playwright's `connectOverCDP` hangs against the device's embedded chromium** -
it never finishes the websocket upgrade (a quirk of the device's `cast_shell`
build; a plain websocket connects fine). Drive it with a raw CDP websocket
instead. Connect to the `page` target from `/json` (the page-level endpoint, not
the browser-level one). This script screenshots the live device and presses a
button; it is the whole loop:

```ts
// drive.ts  ->  run with: bun drive.ts
const targets = (await fetch('http://bridgething.local:9222/json').then(r => r.json())) as any[];
const page = targets.find(t => t.type === 'page');
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise<void>((res, rej) => {
  ws.onopen = () => res();
  setTimeout(() => rej(new Error('ws open timeout')), 8000);
});

let id = 0;
const pending = new Map<number, (v: any) => void>();
ws.onmessage = ev => {
  const m = JSON.parse(ev.data as string);
  if (m.id && pending.has(m.id)) (pending.get(m.id)!(m.result), pending.delete(m.id));
};
const send = (method: string, params: any = {}) =>
  new Promise<any>(res => (pending.set(++id, res), ws.send(JSON.stringify({ id, method, params }))));

// screenshot
const { data } = await send('Page.captureScreenshot', { format: 'png' });
await Bun.write('device.png', Buffer.from(data, 'base64'));

// read what is on screen (great for asserting a change happened)
const text = await send('Runtime.evaluate', { expression: 'document.body.innerText', returnByValue: true });
console.log(text.result.value);

// press a button: keyDown + keyUp. key/code/text must match a real key.
const key = (type: string, k: string, code: string, vk: number) =>
  send('Input.dispatchKeyEvent', { type, key: k, code, text: k.length === 1 ? k : '', windowsVirtualKeyCode: vk });
await key('keyDown', '3', 'Digit3', 51);   // preset 3
await key('keyUp', '3', 'Digit3', 51);

// rotary wheel = horizontal scroll:
await send('Input.dispatchMouseEvent', { type: 'mouseWheel', x: 400, y: 240, deltaX: 120, deltaY: 0 });

process.exit(0);
```

Read the screenshot and the `innerText` to confirm the app reacted. Prefer the
local loop for fast iteration; use the device to confirm real behavior (real
now-playing, real artwork, the actual screen).

## Input reference

The controls arrive as ordinary browser events. Listen for them with a `keydown`
handler and a `wheel` handler on `window`. Make horizontal wheel scroll move
through your primary list; it is the control users reach for first.

| Control      | Browser event                        | CDP `key`/`code`/vk    |
| ------------ | ------------------------------------ | ---------------------- |
| Preset 1-4   | `keydown` key `"1"` `"2"` `"3"` `"4"` | `Digit1`-`Digit4` / 49-52 |
| Mode (M)     | `keydown` key `"m"`                  | `KeyM` / 77            |
| Back         | `keydown` key `"Escape"`             | `Escape` / 27          |
| Rotary wheel | `wheel` with `deltaX` (horizontal)   | mouseWheel `deltaX`    |
| Touch        | pointer / touch events               | `Input.dispatchTouchEvent` |
