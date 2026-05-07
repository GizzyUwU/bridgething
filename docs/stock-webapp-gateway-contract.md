# gateway contract for the stock spotify webapp

This is a reference for gateway-app developers who want their app to
populate the stock Spotify "superbird" webapp on a bridgething-flashed
Car Thing. The stock webapp is the unmodifiable Chromium kiosk app that
ships in the original Spotify firmware; bridgething translates its
interapp calls onto modern wire surfaces, and the gateway answers
those modern surfaces.

A gateway dev does not implement anything stock-specific. The daemon
hides the legacy interapp protocol behind three things you already
implement for any webapp:

1. Answering `library.browse` requests with `BrowseResult`s.
2. Pushing `Player` snapshot/queue/position events.
3. Pushing `Asset.Push` blobs whose ids you reference in the above.

If you do those three things correctly, the stock webapp's home grid,
section drilldowns, podcast pages, now-playing card, queue, presets
panel, and tips overlay all populate. There is no separate "stock
mode" toggle on the gateway side.

## the model

The stock webapp emits "interapp" messages like
`com.spotify.get_children_of_item` or `com.spotify.superbird.graphql`.
The daemon translates those onto modern wire surfaces:

| stock interapp                               | modern surface              | gateway sees                                                                 |
| -------------------------------------------- | --------------------------- | ---------------------------------------------------------------------------- |
| `com.spotify.superbird.get_home`             | `library.browse`            | `LibraryBrowseRequest` with `nodeId=null`                                    |
| `com.spotify.get_children_of_item`           | `library.browse`            | `LibraryBrowseRequest` with a `nodeId`                                       |
| `com.spotify.superbird.get_podcast`          | `library.browse`            | `LibraryBrowseRequest` with show URI                                         |
| `com.spotify.superbird.graphql shelf`        | `library.browse`            | same as `get_home`                                                           |
| `com.spotify.superbird.graphql section`      | `library.browse`            | same as `get_children_of_item`                                               |
| `com.spotify.superbird.play_uri`             | `player.play`               | `PlayUri` command                                                            |
| `com.spotify.queue_spotify_uri`              | `player.queue`              | `QueueUri` command                                                           |
| `com.spotify.set_saved`                      | `library.favoritesSet`      | `FavoritesSet` command                                                       |
| `com.spotify.get_saved`                      | `library.favoritesContains` | `LibraryFavoritesContainsRequest`                                            |
| `com.spotify.set_podcast_playback_speed`     | `player.setSpeed`           | `SetSpeed { speed: 0.5..2.0 }`                                               |
| `com.spotify.get_player_state` (and friends) | n/a (read of cached state)  | nothing - daemon answers from `Player` snapshots you already pushed          |
| `com.spotify.superbird.tts.speak`            | `audio.earcon`              | `Earcon { name: "spotify-stock:<file>" }` (gateway can ignore)               |
| `com.spotify.superbird.earcon`               | `audio.earcon`              | `Earcon { name: "confirmation"\|"listening"\|"error" }` (gateway can ignore) |

Stock graphql calls (`shelf`, `section`, `presets`, `tipsOnDemand`) are
parsed and routed by the daemon. `presets` reads from the local KV
store, `tipsOnDemand` returns canned strings, and `shelf`/`section`
fall back to `library.browse`. The gateway never sees the raw graphql
text.

## library.browse: the heart of it

`LibraryBrowseRequest` and the response shape are defined in
`crates/lib/src/shared/library.rs` (TS bindings at
`crates/lib/ts/bindings/shared.ts`):

```ts
type LibraryBrowseRequest = {
  nodeId: string | null; // null = root (home)
  limit: number; // capped at 100 by the daemon
  offset: number;
};

type BrowseReply = { result: BrowseResult };

type BrowseResult = {
  entries: BrowseEntry[];
  total: number | null; // null if indeterminate
  hasMore: boolean; // authoritative end-of-data signal
};

type BrowseEntry =
  | { type: 'folder'; data: BrowseFolder }
  | { type: 'item'; data: LibraryItem };

type BrowseFolder = {
  nodeId: string; // opaque to daemon - you choose the namespace
  title: string;
  subtitle: string | null;
  artworkId: string | null;
  total: number | null; // count behind this folder, if cheap
  previewChildren: BrowseEntry[] | null; // first-N inline slice
};

type LibraryItem =
  | { type: 'track'; data: Track }
  | { type: 'album'; data: Album }
  | { type: 'playlist'; data: Playlist }
  | { type: 'podcastEpisode'; data: PodcastEpisode }
  | { type: 'show'; data: Show }
  | { type: 'artist'; data: Artist }
  | { type: 'station'; data: Station };
```

In your gateway handler:

```ts
gateway.library.onBrowse(async (handle, req) => {
  if (req.nodeId === null) {
    await handle.respond({ result: await buildHome(req.limit) });
  } else if (req.nodeId.startsWith('spotify:show:')) {
    await handle.respond({
      result: await buildPodcastPage(req.nodeId, req.limit, req.offset),
    });
  } else {
    await handle.respond({
      result: await buildSection(req.nodeId, req.limit, req.offset),
    });
  }
});
```

### the home request

`nodeId === null`, `offset === 0`, `limit` is whatever the webapp asked
for (default 14 for graphql shelf, 20 for legacy `get_home`).

Return a `BrowseResult` whose `entries` are **all `Folder`s with
`previewChildren` populated**. Each top-level folder is a home shelf;
its `previewChildren` are the cards rendered in that shelf without a
drilldown.

```ts
{
  entries: [
    { type: 'folder', data: {
      nodeId: 'home:recently-played',
      title: 'Recently played',
      subtitle: null,
      artworkId: null,
      total: 50,
      previewChildren: [
        { type: 'folder', data: { nodeId: 'spotify:playlist:abc', title: 'Discover Weekly', subtitle: 'Spotify', artworkId: 'art:dw', total: null, previewChildren: null }},
        { type: 'item', data: { type: 'track', data: { id: 'spotify:track:xyz', name: '...', artist: { id: '...', name: '...' }, ... }}},
      ],
    }},
    { type: 'folder', data: { nodeId: 'home:made-for-you', title: 'Made for you', ... }},
  ],
  total: null,
  hasMore: false,
}
```

When the user taps "see all" or scrolls past the preview slice, the
webapp re-issues `library.browse` with the shelf's `nodeId` as
`parentId` plus `offset`. Make `nodeId` self-routing back to the same
shelf logic.

### the section / drilldown request

`nodeId` is something you returned in a previous `BrowseResult` - either
a synthetic shelf id (`home:recently-played`) or a Spotify URI
(`spotify:playlist:...`). Return entries appropriate to that node.

Pagination: webapps raise `offset` until `hasMore` is `false`. Set
`total` if you can cheaply expose it (the webapp uses it for the "X of
Y" indicator); leave it null otherwise.

### the podcast detail request

When the user opens a podcast show, the webapp invokes
`getPodcast(uri, limit, offset)` which lands as
`LibraryBrowseRequest { nodeId: showUri, ... }`. Return the show's
episodes as `LibraryItem::PodcastEpisode` items. There is no special
"podcast mode" - it's just a section drilldown that happens to return
episodes.

The legacy stock response shape carries a `consumption_order`
(`'RECENT' | 'EPISODIC'`) and a `latest_played_uri` field; bridgething
synthesizes those from the episode order you return and from
`PlayerState.context_uri`. You do not need to populate them.

## node id patterns the webapp will round-trip

The webapp does not invent node ids. It only echoes back what you sent.
That means **your gateway picks the namespace.** Three rules:

1. **Use Spotify URIs for anything playable.** Tracks, albums,
   playlists, shows, artists, podcast episodes - emit them with their
   real `spotify:<kind>:<base62>` URI as the `id`/`uri` field. The
   webapp's `playUri` round-trip then works against the real Spotify
   service via your gateway's `Player.Play` handler.
2. **Use synthetic ids for shelves and pseudo-folders.** Anything that
   is not a real playable entity - "Recently played", a curated rail,
   a featured space - gets a gateway-defined synthetic id. Pick a
   prefix (`home:`, `gw:`, `mygateway:section:`) and route on it
   server-side. The daemon does not interpret these.
3. **One special case the daemon will hand you verbatim:** the legacy
   stock `getRecentlyPlayed()` helper hardcodes `parent_id =
'spotify:recently-played'`. If you want recently-played to work via
   the legacy non-graphql code path, route that pseudo-URI specifically.
   In modern graphql mode, the webapp uses your home shelf's `nodeId`
   instead, so this only matters if the device's `graphQLShelfEnabled`
   remote config flag is off.

The webapp never inspects URI structure beyond a few prefix checks
(`spotify:playlist:`, `spotify:show:`, `spotify:section:`, etc.) for
choosing icons and render paths. Anything else is opaque.

## play_uri and feature_identifier

When the user taps a card, the webapp invokes
`playUri(uri, featureIdentifier, interactionId?, skipToUri?, skipToUid?)`.
The daemon translates that to `Player.Play` with the URI. Your gateway
then resolves the URI on Spotify's side and starts playback.

The `featureIdentifier` is for analytics on the Spotify side and
indicates which surface initiated playback. Stock-webapp values:

- `car-thing-content-shelf` - tap on home shelf or section
- `car-thing-album` / `car-thing-artist` / `car-thing-playlist` /
  `car-thing-podcast` / `car-thing-entity-page` - tap inside an entity
  detail page
- `car-thing-voice` - voice search result
- `car-thing-show-me` / `car-thing-alternative-search` - voice intents
- `car-thing-presets` - preset button press

bridgething forwards `feature_identifier` on `Player.Play` so you can
log it. For most gateways this is a leaf field you ignore; it matters
only if you mirror Spotify-style analytics.

## player state: what the now-playing card reads

Push player snapshots through `Player.Snapshot`, `Player.QueueChanged`,
and `Player.PositionChanged` whenever your side state changes. The
daemon caches the merged result and answers stock's
`get_player_state` / `get_current_track` / `get_current_context` /
`get_track_elapsed` / `superbird.player_state` / `get_session_state`
reads from cache.

Fields the stock UI actually reads (don't worry about populating
others):

- `track.uri`, `track.name`, `track.artist.name`, `track.album.name` -
  now-playing strings.
- `track.image_id` - the artwork token. Push the bytes via
  `Asset.Push` keyed by this id (see [artwork](#artwork) below).
- `track.duration_ms`, `track.is_explicit`, `track.is_episode` (for
  podcast UI), `track.episode.show_uri` (for podcast back-button).
- `context_uri` - drives the "playing from <X>" line and
  saved/preset highlighting.
- `is_paused` - the play/pause button state.
- `playback_position` (or whatever your latest `PositionChanged`
  posted) - the seek bar.
- `playback_options.shuffle`, `playback_options.repeat` - mode toggles.
- `playback_restrictions.{can_skip_next, can_skip_prev, can_seek,
can_toggle_shuffle, can_toggle_repeat}` - the UI greys out unavailable
  buttons. **Send these honestly.** If you don't, the UI lets the user
  press buttons that no-op silently.
- `playback_speed` (podcast only) - 0.5 / 0.8 / 1.0 / 1.2 / 1.5 / 2.0.
  Stock encodes as integer percent (50/80/100/...) on the wire; the
  daemon converts. Push as a float multiplier from your side.
- `currently_active_application` - if set, the webapp shows a "playing
  via <app>" banner and disables transport. Use it when audio is
  routed through something other than your gateway.

`playback_id` and `options.repeating_track` / `options.shuffling_context`
exist on the stock wire but the stock webapp does not read them.

## queue

Push `Player.QueueChanged` with the upcoming items whenever the queue
changes. The webapp reads `get_next_tracks` from cache and renders the
"Up next" panel from that.

## artwork

Every artwork field in your `BrowseResult` / `PlayerState` /
`Preset` is a string token that the daemon resolves through its
`AssetCache`. The webapp asks for the bytes via
`get_image(id)` (~248 px) or `get_thumbnail_image(id)` (~96 px); the
daemon serves from cache.

Populate the cache by sending `Asset.Push` for small images
(<=16 KB on the Bulk lane) or `Asset.Begin` + `Asset.Chunk` +
`Asset.Commit` for larger ones. The id you push under is the same
string you placed in `artworkId` / `imageId` / `image_id`.

For the now-playing iPhone case, the daemon pulls artwork from iAP2's
`ArtworkTransfer` channel and assigns ids of the form
`iap2/art/<persistent_hex>/<transfer_id>` automatically. **Do not push
artwork under those ids from the gateway** - they belong to the iAP2
path. Use your own namespace (e.g., `gw:art:<hash>`) for everything you
push.

The webapp re-requests by id when a card scrolls into view, so you do
not need to pre-populate; pushing on demand (the moment you emit a
`BrowseResult` referencing a new id) is fine. Repeated requests for the
same id are cache hits in the daemon.

A few special prefixes the webapp recognizes:

- `:localfileimage:` substring anywhere in the id - treat as opaque,
  daemon doesn't apply any special handling.
- Otherwise the id is fully opaque.

## presets

Stock has 4 preset slots (1..4). The user sets them by long-pressing
the physical preset buttons; you do **not** populate or write presets
from the gateway. Presets persist in the daemon's KV store under
`Uuid::nil()` scope at keys `presets:1`..`presets:4` (the same store
that `client.store` writes through), so a debug webapp can inspect or
seed them via `Store.Get { key: "presets:1" }` if needed.

Preset shape (`crates/lib/src/stock/mod.rs::StockPreset`):

```ts
type StockPreset = {
  contextUri: string; // a spotify: URI you'll receive on play_uri
  imageUrl: string | null;
  slotIndex: number; // 1..4
  name: string | null;
  description: string | null;
};
```

When the user presses a preset button, the daemon emits
`Player.Play { uri: contextUri, feature_identifier: 'car-thing-presets' }`
through your gateway. The companion app, not the gateway, is generally
where preset metadata enrichment (name / description / image_url)
happens; the daemon does not mediate it. If you want presets pre-seeded
in tests, write them directly via `client.store`.

## tips

The "tips and tricks" overlay reads canned strings out of the daemon
(`crates/core/src/handler/client/stock.rs::canned_tips`). The gateway
plays no role.

## what stock asks but you can ignore

Stock declares (in `superbird-webapp/src/middleware/InterappActions.ts`)
several interapp method names that no caller in the webapp ever
invokes. The daemon swallows them as no-ops. Your gateway will never
see anything on these paths:

- `com.spotify.search_query`
- `com.spotify.get_recommended_content_for_type`
- `com.spotify.get_root_item`
- `com.spotify.get_items_for_uris`
- `com.spotify.get_capabilities`
- `com.spotify.get_shuffle`, `get_repeat`, `get_playback_speed`,
  `get_crossfade_state`, `get_podcast_playback_speed`,
  `get_available_podcast_playback_speeds`
- `com.spotify.start_radio`, `set_rating`, `get_rating`
- `com.spotify.superbird.phone.send_message`
- All of the legacy `_play_item`, `_play_uri`, `_skip_next`,
  `_skip_previous`, `_seek_to_position`, `_set_playback_speed`,
  `_set_repeat`, `_set_shuffle` underscore-prefixed names

Don't bother implementing handlers for these. The modern Library
surface has `search` and `recommendations` request types, but stock
will never trigger them; those are for third-party webapps.

Stock also asks for `tts.speak` with file paths like
`PRESET_UNAVAILABLE.mp3`. The daemon translates these to `audio.earcon`
events with `name: "spotify-stock:PRESET_UNAVAILABLE.mp3"`. The Car
Thing has no speakers, so the gateway is the only thing that could
play these - but the file paths are opaque, the audio assets aren't
shipped, and you almost certainly want to drop them. If you want
parity with Spotify's stock car experience, intercept the prefixed
earcon names on your side and play your own short feedback sound.

## a minimal end-to-end smoke test

To verify your gateway populates the stock webapp:

1. Subscribe to `library.browse`. On a `nodeId === null` request,
   respond with one folder containing one preview track. The webapp
   should render a single shelf on home with one card.
2. Push an `Asset.Push` with the artwork id you used. The card should
   show your image.
3. Subscribe to `Player.Play`. When the user taps the card, you'll
   receive a `PlayUri { uri }` command. Push a `Player.Snapshot` with
   `track`, `context_uri`, `is_paused: false`, and a sane
   `playback_restrictions`. The now-playing card should populate.
4. Push `PositionChanged` events at ~1 Hz. The seek bar should advance.

If those four work, the rest of the surface area is shape-matching
exercises against `BrowseResult` and `PlayerState`.

## related references

- `crates/lib/src/shared/library.rs` - canonical wire types.
- `crates/lib/src/shared/player.rs` - `PlayerState`, `Track`,
  `PlaybackOptions`, `PlaybackRestrictions`, etc.
- `crates/lib/src/shared/asset.rs` - asset primitives.
- `crates/core/src/handler/client/stock.rs` - daemon-side translation
  layer; read this if you're debugging why the stock webapp behaves
  oddly with your `BrowseResult`.
- `crates/core/src/stock/interapp.rs` - the inbound interapp →
  modern-surface translation and the dead-method swallow list.
- `superbird-webapp/src/middleware/InterappActions.ts` - the original
  stock interapp API surface, for reference when something on the
  Chromium side surprises you.
