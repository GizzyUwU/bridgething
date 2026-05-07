# **PROJECT_NAME**

A bridgething webapp scaffolded with `create-bridgething`.

Stack: React 19 + Vite + Tailwind v4 + TypeScript strict, with
[`@bridgething/client`](https://github.com/JoeyEamigh/bridgething) preinstalled.

## Develop

```bash
bun install
bun run dev
```

Opens at http://localhost:5173/. The starter `App.tsx` connects to
`ws://<host>/` (the daemon's local WebSocket on port 8891 of the device
itself). To dev against a remote device, set:

```bash
VITE_BRIDGETHING_URL=ws://<device-ip>:8891/ bun run dev
```

## Push to a device

```bash
bun run build
bun run push <device-ip>
```

`push` rsyncs `dist/` into `/var/bridgething/webapps/__PROJECT_NAME__/`
on the Car Thing.

## Layout

- `src/App.tsx` - starter UI: subscribes to `client.player.onPlayerState`,
  fetches artwork via `client.asset.get`, exposes transport controls.
- `src/index.css` - `@import "tailwindcss";` (Tailwind v4 CSS-first).
- `vite.config.ts` - vite + tailwind plugin, `es2022` target.
- `index.html` - 800x480 viewport, no overscroll, no tap highlight.
