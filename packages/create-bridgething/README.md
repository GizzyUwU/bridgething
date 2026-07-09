# create-bridgething

Scaffold a new [bridgething](https://github.com/JoeyEamigh/bridgething) webapp, a
single-page app that runs on the kiosk chromium of a Spotify Car Thing and talks
to the on-device daemon. The template ships React + Vite + Tailwind v4 with
[`@bridgething/client`](https://www.npmjs.com/package/@bridgething/client)
preinstalled.

```sh
bun create bridgething my-app
```

```sh
npm create bridgething@latest my-app
```

The generated project includes `bun run push` to deploy the built webapp to a
connected Car Thing, `bun run share` to share it with the community, and
`bun run update` to bring the connected device to the latest bridgething release.

- Full docs: <https://bridgething.com/docs>
- Source: <https://github.com/JoeyEamigh/bridgething>
