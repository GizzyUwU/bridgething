# bridgething.com

Marketing site, release notes, and discover-manifest infra for bridgething.

## what's in here

```
src/                Astro static site (landing, /docs, /releases pages).
manifest/           Discover-manifest types, generator, validator, golden tests.
sdk/                Webapp SDK reference pipeline: copies the codegen-emitted
                    surfaces.json from the bridgething repo, rendered at /docs.
apps/               App catalog (catalog.v1): generator, tests, apps.yaml
                    (human curation) and apps-published.yaml (CI state).
worker/             Source-directory API backing /apps/store: submission,
                    moderation, the published catalog.v1 directory doc, and the
                    edge-cached catalog relay for listed sources.
scripts/            Bundle composer (image .zip + settings.ext4 -> bundle .zip).
public/brand/       Mode-aware logo + lockup + tagline assets.
public/fonts/       Self-hosted Outfit + Inter (SIL OFL).
wrangler.toml       Cloudflare Workers static-assets deploy config.
```

## local dev

```sh
bun install
bun run manifest:build   # writes manifest/fixture.json from manifest/bundles.yaml
bun run dev              # astro dev server
bun run check            # astro + tsc type-check
bun test                 # manifest generator + validator tests
bun run build            # astro build to dist/
```

By default `manifest:build` reads release files from sibling repos:

- `BRIDGETHING_REPO` (default `../bridgething`)
- `YOCTO_SUPERBIRD_REPO` (default `../yocto-superbird`)

Override via env vars when CI checks out elsewhere.

`manifest/bundles.yaml` lists which (daemon, image) pairs CI has bundled. With
zero bundles, the build skips writing the fixture and `/releases` renders an
empty state.

### the source-directory API

`astro dev` serves only the static site; `/api/*` is the Worker, so it needs
wrangler:

```sh
bun run build
bunx wrangler dev --local        # serves dist/ plus /api/*, with a local KV
```

Put `ADMIN_TOKEN=<anything>` in `.dev.vars` (gitignored) to reach the moderation
routes and `/admin/sources`. Seed the local KV directly to exercise the tiers
without submitting real sources:

```sh
bunx wrangler kv key put --local --binding SOURCES "source:<url>" --path record.json
```

The catalog schema, its validator and the resolution rules come from the
`@bridgething/catalog` workspace package. The validator is generated and
committed there because the Workers runtime refuses `new Function`, so the
schema cannot be compiled at request time; regenerate it with
`bun run --cwd ../packages/catalog validator`.

## deploy

```sh
bun run deploy   # wrangler deploy (account_id baked in wrangler.toml)
```

This deploys the Astro static build to a Cloudflare Worker with the
`bridgething.com` and `www.bridgething.com` custom domains. Account is set;
you still need:

- The R2 bucket created out-of-band: `bunx wrangler r2 bucket create bridgething-ota`
- `ota.bridgething.com` attached to the bucket as a custom domain (Cloudflare
  dashboard or `bunx wrangler r2 bucket domain add bridgething-ota --domain ota.bridgething.com`).
- The app-catalog bucket: `bunx wrangler r2 bucket create bridgething-apps` with
  `apps.bridgething.com` attached the same way. The `publish-apps` workflow
  uploads `catalog.json`, `r/<uuid>/<version>.zip`, and `icons/<uuid>.<ext>`.
- The source-directory KV namespace, whose id goes in `wrangler.toml`:
  `bunx wrangler kv namespace create SOURCES`. **Deploy fails until this exists**,
  because `wrangler.toml` ships a placeholder id.
- The moderation secret: `bunx wrangler secret put ADMIN_TOKEN`.

Manifest, catalog, and artifacts are served directly from R2; the Worker only
renders the marketing site.

## release flow

Bump a version, CI dispatches `bundle`, the composer runs, the manifest
regenerates, R2 takes the new bytes and the new state.
