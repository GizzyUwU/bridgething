//! sfp: host cli for the spotify client. validates the wire surface
//! against a live account using the worker-psk env contract.
//!
//!   SPOTIFY_AUTH_PSK              (required) gates the private-auth worker
//!   SPOTIFY_AUTH_BASE            worker base url (default thinglabs.sh/auth)
//!   SPOTIFY_PRIVATE_STATE        state dir (default .spotify-private)
//!   SPOTIFY_CARTHING_REFRESH_TOKEN   optional: seed a token instead of pairing
//!   SPOTIFY_USERNAME             optional: canonical username for library calls
//!
//! commands: probe (default) | pair | np | home | search <q> | devices |
//!           pause | resume | next | prev | seek <ms> | play <uri>

use std::{error::Error, path::PathBuf, sync::Arc, time::Duration};

use librespot_protocol::{
  client_info::ClientInfo,
  connect::Cluster,
  credentials::OneTimeToken,
  login5::{LoginRequest, LoginResponse, login_request::Login_method, login_response::Response as Login5Response},
};
use protobuf::{Message, MessageField};
use spotify::{
  auth::{Auth, DEFAULT_WORKER_BASE, TokenStore},
  client::{Observer, SpotifyClient},
  dealer::{Dealer, active_device},
  http::{ANDROID_CLIENT_ID, SPCLIENT, SpHttp, random_hex},
  model::{AuthState, Device, LibraryScope, PlayerState, Queue},
  spclient::SpClient,
  store::{FileTokenStore, load_or_make_device_id},
  util::image_hex,
};

type Boxed = Box<dyn Error>;

#[tokio::main]
async fn main() -> Result<(), Boxed> {
  let args: Vec<String> = std::env::args().collect();
  let cmd = args.get(1).map(String::as_str).unwrap_or("probe");

  let psk =
    std::env::var("SPOTIFY_AUTH_PSK").map_err(|_| "SPOTIFY_AUTH_PSK is required (gates the private-auth worker)")?;
  let base = std::env::var("SPOTIFY_AUTH_BASE").unwrap_or_else(|_| DEFAULT_WORKER_BASE.to_string());
  let state_dir =
    PathBuf::from(std::env::var("SPOTIFY_PRIVATE_STATE").unwrap_or_else(|_| ".spotify-private".to_string()));
  let username = std::env::var("SPOTIFY_USERNAME").ok();

  let store = FileTokenStore::new(&state_dir)?;
  if let Ok(seed) = std::env::var("SPOTIFY_CARTHING_REFRESH_TOKEN")
    && !seed.is_empty()
    && store.load_refresh_token().is_none()
  {
    store.save_refresh_token(seed);
  }
  let device_id = load_or_make_device_id(&state_dir);
  let auth = Arc::new(Auth::new(base, psk, Box::new(store)));

  if !auth.is_paired().await {
    eprintln!("not paired; starting device-code flow...");
    pair(&auth).await?;
  }

  let http = SpHttp::new(auth.clone());
  let spc = SpClient::new(http.clone());
  let dealer = Dealer::new(http.clone(), device_id);

  let username = match username {
    Some(u) => Some(u),
    None => spotify::aplogin::resolve_and_cache(auth.as_ref(), &http.http, dealer.device_id())
      .await
      .ok(),
  };

  match cmd {
    "pair" => println!("paired."),
    "probe" => probe(&spc, &dealer, username.as_deref()).await?,
    "np" => {
      let (_stream, writer) = dealer.open().await?;
      println!("{}", describe_np(&writer.cluster().await?));
    }
    "home" => {
      let home = spc.get_home("en").await?;
      println!("home: {} sections", home.body.sections.len());
      for s in home.body.sections.iter().take(20) {
        let car = pick_carousel(s);
        println!("  [{}] {:?}  {} items", section_kind(s), car.0, car.1);
      }
    }
    "search" => {
      let q = args.get(2).map(String::as_str).unwrap_or("daft punk");
      print_search(&spc, q).await?;
    }
    "devices" => {
      let (_stream, writer) = dealer.open().await?;
      let cluster = writer.cluster().await?;
      for (id, info) in &cluster.device {
        let active = if *id == cluster.active_device_id {
          " *active*"
        } else {
          ""
        };
        println!(
          "  {} [{:?}] vol={}{}",
          info.name,
          info.device_type.enum_value_or_default(),
          info.volume,
          active
        );
      }
    }
    "watch" => {
      let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
      let client = SpotifyClient::new(auth.clone(), dealer.device_id().to_string(), Arc::new(PrintObserver));
      client.connect().await?;
      println!("watching {secs}s - play/pause/skip on Spotify to see deltas...");
      tokio::time::sleep(Duration::from_secs(secs)).await;
      client.disconnect().await;
    }
    "product" => {
      let client = SpotifyClient::new(auth.clone(), dealer.device_id().to_string(), Arc::new(PrintObserver));
      let p = client.product().await?;
      println!(
        "product={} catalogue={} country={} premium={} can_use_superbird={}",
        p.product, p.catalogue, p.country, p.is_premium, p.can_use_superbird
      );
    }
    "root" => {
      let client = SpotifyClient::new(auth.clone(), dealer.device_id().to_string(), Arc::new(PrintObserver));
      client.connect().await?;
      let shelves = client.root_browse().await?;
      println!("root: {} shelves", shelves.len());
      for s in &shelves {
        println!("  [{}] {:?}  {} items", s.id, s.title, s.items.len());
      }
      client.disconnect().await;
    }
    "lib" => {
      let node = args.get(2).map(String::as_str).unwrap_or("playlists");
      let client = SpotifyClient::new(auth.clone(), dealer.device_id().to_string(), Arc::new(PrintObserver));
      client.connect().await?;
      let page = client.browse(node, 20, 0).await?;
      println!(
        "browse {node:?}: {} items (total={:?} more={})",
        page.items.len(),
        page.total,
        page.has_more
      );
      for it in page.items.iter().take(20) {
        println!(
          "  {} - {} [{}] art={}",
          it.title,
          it.subtitle,
          kind_of(&it.uri),
          it.image_id
        );
      }
      client.disconnect().await;
    }
    "fav" => {
      let client = SpotifyClient::new(auth.clone(), dealer.device_id().to_string(), Arc::new(PrintObserver));
      client.connect().await?;
      let page = client.favorites_list(20, 0).await?;
      println!("favorites: {} items (total={:?})", page.items.len(), page.total);
      for it in page.items.iter().take(10) {
        println!("  {} - {} saved={}", it.title, it.subtitle, it.saved);
      }
      client.disconnect().await;
    }
    "whoami" => whoami(&http).await?,
    "apwhoami" => {
      let bearer = http.auth.bearer().await?;
      match spotify::aplogin::resolve_username(&http.http, &bearer, dealer.device_id()).await {
        Ok(u) => println!("canonical username = {u}"),
        Err(e) => println!("AP login failed: {e}"),
      }
    }
    "pause" | "resume" | "next" | "prev" | "seek" | "play" => {
      write_cmd(&dealer, cmd, args.get(2).map(String::as_str)).await?
    }
    other => {
      eprintln!("unknown command: {other}");
      std::process::exit(2);
    }
  }
  Ok(())
}

struct PrintObserver;

impl Observer for PrintObserver {
  fn on_player(&self, s: PlayerState) {
    let track = match &s.track {
      Some(t) => format!(
        "{} - {}",
        t.name,
        t.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
      ),
      None => "(nothing)".to_string(),
    };
    println!(
      "[player] {track} | {} | {}s/{}s | shuffle={} repeat={:?}",
      if s.is_paused { "paused" } else { "playing" },
      s.position_ms / 1000,
      s.duration_ms / 1000,
      s.shuffle,
      s.repeat,
    );
  }
  fn on_queue(&self, q: Queue) {
    println!("[queue] {} upcoming", q.next.len());
  }
  fn on_devices(&self, d: Vec<Device>) {
    let names: Vec<String> = d
      .iter()
      .map(|x| format!("{}{}", x.name, if x.is_active { "*" } else { "" }))
      .collect();
    println!("[devices] {}", names.join(", "));
  }
  fn on_auth(&self, a: AuthState) {
    println!("[auth] {a:?}");
  }
  fn on_library_changed(&self, scope: LibraryScope) {
    println!("[library] changed: {scope:?}");
  }
}

async fn whoami(http: &SpHttp) -> Result<(), Boxed> {
  let bearer = http.auth.bearer().await?;

  println!("== product_state (full) ==");
  let resp = http
    .http
    .get(format!("{SPCLIENT}/melody/v1/product_state"))
    .headers(http.headers(true).await?)
    .send()
    .await?;
  let body = resp.text().await?;
  println!("{body}");

  println!("\n== login5 one_time_token = access_token ==");
  let mut ci = ClientInfo::new();
  ci.client_id = ANDROID_CLIENT_ID.to_string();
  ci.device_id = random_hex(20);
  let mut ott = OneTimeToken::new();
  ott.token = bearer.clone();
  let mut req = LoginRequest::new();
  req.client_info = MessageField::some(ci);
  req.login_method = Some(Login_method::OneTimeToken(ott));
  let resp = http
    .http
    .post("https://login5.spotify.com/v3/login")
    .header(reqwest::header::ACCEPT, "application/x-protobuf")
    .header(reqwest::header::CONTENT_TYPE, "application/x-protobuf")
    .body(req.write_to_bytes()?)
    .send()
    .await?;
  let status = resp.status();
  let bytes = resp.bytes().await?;
  println!("  login5 -> {status} ({} bytes)", bytes.len());
  match LoginResponse::parse_from_bytes(&bytes) {
    Ok(lr) => match lr.response {
      Some(Login5Response::Ok(ok)) => println!("  OK username = {}", ok.username),
      Some(Login5Response::Error(e)) => println!("  error = {e:?}"),
      Some(Login5Response::Challenges(_)) => println!("  challenges (would need client-token + hashcash)"),
      Some(_) => println!("  (other login5 response variant)"),
      None => println!("  no response variant; warnings={:?}", lr.warnings),
    },
    Err(_) => println!(
      "  unparseable: {}",
      String::from_utf8_lossy(&bytes).chars().take(160).collect::<String>()
    ),
  }
  Ok(())
}

async fn pair(auth: &Auth) -> Result<(), Boxed> {
  let flow = auth.begin_device_flow().await?;
  println!("\n  open: {}\n  code: {}\n", flow.verification_uri, flow.user_code);
  println!("waiting for approval...");
  auth.complete_device_flow(&flow).await?;
  println!("paired ok.");
  Ok(())
}

async fn probe(spc: &SpClient, dealer: &Dealer, username: Option<&str>) -> Result<(), Boxed> {
  println!("== product_state ==");
  let product = spc.product_state().await?;
  let pick = |k: &str| product.get(k).and_then(|v| v.as_str()).unwrap_or("?").to_string();
  println!(
    "  product={} catalogue={} country={} on-demand={}",
    pick("product"),
    pick("catalogue"),
    pick("country"),
    pick("on-demand"),
  );

  println!("== dealer cluster ==");
  let (_stream, writer) = dealer.open().await?;
  let cid = writer.connection_id().to_string();
  println!("  connection-id: {}...", &cid[..cid.len().min(20)]);
  let cluster = writer.cluster().await?;
  println!(
    "  active_device_id: {}",
    if cluster.active_device_id.is_empty() {
      "(none)"
    } else {
      &cluster.active_device_id
    }
  );
  println!("  now-playing: {}", describe_np(&cluster));
  println!("  devices: {}", cluster.device.len());

  let np_uri = cluster.player_state.track.uri.clone();
  if np_uri.starts_with("spotify:track:") {
    println!("== hydrate now-playing track ==");
    let tracks = spc.get_tracks(std::slice::from_ref(&np_uri)).await?;
    if let Some(t) = tracks.get(&np_uri) {
      let artists: Vec<&str> = t.artist.iter().map(|a| a.name()).collect();
      println!(
        "  {} - {} [{}s] art={}",
        t.name(),
        artists.join(", "),
        t.duration() / 1000,
        image_hex(&t.album.cover_group)
      );
    }
  }

  println!("== casita home ==");
  let home = spc.get_home("en").await?;
  let populated = home.body.sections.iter().filter(|s| pick_carousel(s).1 > 0).count();
  println!("  {} sections ({} populated)", home.body.sections.len(), populated);
  for s in home.body.sections.iter().filter(|s| pick_carousel(s).1 > 0).take(8) {
    let (title, n) = pick_carousel(s);
    println!("    [{}] {:?}  {} items", section_kind(s), title, n);
  }

  println!("== search 'daft punk' ==");
  print_search(spc, "daft punk").await?;

  if let Some(user) = username {
    println!("== user library ({user}) ==");
    match spc.rootlist(user).await {
      Ok(rl) => println!("  rootlist: {} items", rl.contents.items.len()),
      Err(e) => println!("  rootlist FAILED: {e}"),
    }
    match spc.collection_paging(user, "collection", 50).await {
      Ok(items) => println!("  liked collection: {} items", items.len()),
      Err(e) => println!("  collection FAILED: {e}"),
    }
    match spc.recently_played(user, 20).await {
      Ok(rp) => println!("  recently-played: {} items", rp.items.len()),
      Err(e) => println!("  recently-played FAILED: {e}"),
    }
  } else {
    println!("(set SPOTIFY_USERNAME to probe rootlist/collection/recents)");
  }

  println!("\nprobe complete.");
  Ok(())
}

async fn print_search(spc: &SpClient, q: &str) -> Result<(), Boxed> {
  let res = spc.search(q, 6).await?;
  let mut shown = 0;
  for it in &res.items {
    if shown >= 8 {
      break;
    }
    if !it.section.entries.is_empty() {
      for e in &it.section.entries {
        let ent = &e.item.entity;
        println!("  [section] {} {}", kind_of(&ent.uri), ent.name);
        shown += 1;
      }
    } else if !it.uri.is_empty() {
      println!("  {} {}", kind_of(&it.uri), it.name);
      shown += 1;
    }
  }
  Ok(())
}

async fn write_cmd(dealer: &Dealer, cmd: &str, arg: Option<&str>) -> Result<(), Boxed> {
  let (_stream, writer) = dealer.open().await?;
  let cluster = writer.cluster().await?;
  let target = active_device(&cluster, dealer.device_id()).ok_or("no reachable target device")?;
  let (status, body) = match cmd {
    "pause" => writer.pause(&target).await?,
    "resume" => writer.resume(&target).await?,
    "next" => writer.skip_next(&target).await?,
    "prev" => writer.skip_prev(&target).await?,
    "seek" => {
      let ms: i64 = arg.unwrap_or("0").parse().unwrap_or(0);
      writer.seek_to(&target, ms).await?
    }
    "play" => {
      let uri = arg.ok_or("play needs a context uri")?;
      writer.play(&target, play_envelope(uri)).await?
    }
    _ => unreachable!(),
  };
  println!("{cmd} -> {status} {}", body.chars().take(120).collect::<String>());
  Ok(())
}

fn play_envelope(uri: &str) -> serde_json::Value {
  serde_json::json!({
      "endpoint": "play",
      "context": {"uri": uri, "url": format!("context://{uri}"), "metadata": {}},
      "play_origin": {"feature_identifier": "harmony", "feature_version": "9.1.52.1394", "referrer_identifier": "home"},
      "prepare_play_options": {"license": "premium"},
      "play_options": {"reason": "interactive", "operation": "replace", "trigger": "immediately"},
  })
}

fn describe_np(cluster: &Cluster) -> String {
  let ps = &cluster.player_state;
  let uri = ps.track.uri.clone();
  if uri.is_empty() {
    return "(nothing playing)".to_string();
  }
  let md = &ps.track.metadata;
  let title = md.get("title").cloned().unwrap_or_default();
  let artist = md
    .get("artist_name")
    .cloned()
    .unwrap_or_else(|| "(artist via hydration)".to_string());
  format!(
    "{title} - {artist} [{}] {uri}",
    if ps.is_paused { "paused" } else { "playing" }
  )
}

fn pick_carousel(s: &spotify::proto::custom::casita_home::Section) -> (String, usize) {
  for car in [&s.shortcuts, &s.carousel, &s.list_carousel] {
    if let Some(c) = car.as_ref() {
      let n = c.items.inner.items.len();
      if n > 0 || !c.header.title.text.is_empty() {
        return (c.header.title.text.clone(), n);
      }
    }
  }
  (String::new(), 0)
}

fn section_kind(s: &spotify::proto::custom::casita_home::Section) -> String {
  let uri = &s.id.uri;
  uri.rsplit('|').next().unwrap_or(uri).to_string()
}

fn kind_of(uri: &str) -> &str {
  uri.split(':').nth(1).unwrap_or("?")
}
