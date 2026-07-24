use std::{
  collections::HashMap,
  time::{SystemTime, UNIX_EPOCH},
};

use librespot_protocol::{
  extended_metadata::{
    BatchedEntityRequest, BatchedEntityRequestHeader, BatchedExtensionResponse, EntityRequest, ExtensionQuery,
  },
  extension_kind::ExtensionKind,
  metadata::{Album, Artist, Episode, Show, Track},
  playlist4_external::SelectedListContent,
};
use protobuf::{Message, MessageField};
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderValue};

use crate::{
  error::{Error, Result},
  http::{SPCLIENT, SpHttp, random_hex},
  httpx::{HttpMethod, with_query},
  proto::custom::{
    casita_home::HomeResponse,
    collection::{Item as CollectionWriteItem, WriteRequest},
    recently_played::RecentlyPlayed,
    searchview::SearchResponse,
  },
  util::{self, CollectionItem},
};

const COLLECTION_CT: &str = "application/vnd.collection-v2.spotify.proto";
const SEARCH_TYPES: &str = "album,artist,genre,playlist,user_profile,track,show,audio_episode,\
audiobook,section,author,concert,venue,podcast_chapter";
const SEARCH_FEATURES: &str = "abdesc,fullflatfilterlist,vidfilter,recsection,track_classification,\
sectioner,pl_spotify_logic,trackversions,showverified";

fn now_secs() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0)
}

fn normalize(ids: &[String], kind: &str) -> Vec<String> {
  ids
    .iter()
    .map(|i| {
      if i.starts_with("spotify:") {
        i.clone()
      } else {
        format!("spotify:{kind}:{i}")
      }
    })
    .collect()
}

#[derive(Clone)]
pub struct SpClient {
  http: SpHttp,
}

impl SpClient {
  pub fn new(http: SpHttp) -> Self {
    SpClient { http }
  }

  async fn get_proto<T: Message>(&self, url: String, query: &[(&str, String)], what: &str) -> Result<T> {
    tracing::debug!(%what, "spclient: get");
    let headers = self.http.headers(false).await?;
    let resp = self
      .http
      .send(HttpMethod::Get, with_query(url, query)?, headers, Vec::new(), 0)
      .await?;
    if !resp.ok() {
      return Err(Error::status(what, resp.status, resp.text()));
    }
    Ok(T::parse_from_bytes(&resp.body)?)
  }

  async fn post_proto<T: Message>(&self, url: String, body: Vec<u8>, what: &str) -> Result<T> {
    tracing::debug!(%what, "spclient: post");
    let headers = self.http.headers(false).await?;
    let resp = self.http.send(HttpMethod::Post, url, headers, body, 0).await?;
    if !resp.ok() {
      return Err(Error::status(what, resp.status, resp.text()));
    }
    Ok(T::parse_from_bytes(&resp.body)?)
  }

  pub async fn product_state(&self) -> Result<serde_json::Value> {
    let headers = self.http.headers(true).await?;
    let resp = self
      .http
      .send(
        HttpMethod::Get,
        format!("{SPCLIENT}/melody/v1/product_state"),
        headers,
        Vec::new(),
        0,
      )
      .await?;
    if !resp.ok() {
      return Err(Error::status("product_state", resp.status, resp.text()));
    }
    Ok(serde_json::from_slice(&resp.body)?)
  }

  pub async fn get_home(&self, locale: &str) -> Result<HomeResponse> {
    self
      .get_proto(
        format!("{SPCLIENT}/casita/v1/home/default"),
        &[("locale", locale.to_string())],
        "casita_home",
      )
      .await
  }

  pub async fn search(&self, query: &str, limit: u32) -> Result<SearchResponse> {
    let q = vec![
      ("request_id", random_hex(16)),
      ("query", query.to_string()),
      ("locale", "en_US".to_string()),
      ("entity_types", SEARCH_TYPES.to_string()),
      ("timestamp", util::now_ms().to_string()),
      ("limit", limit.to_string()),
      ("page_token", String::new()),
      ("show_type", "podcast".to_string()),
      ("query_complete", "submit".to_string()),
      ("album_states", "live,prerelease".to_string()),
      ("audiobook_states", "live,prerelease".to_string()),
      ("features", SEARCH_FEATURES.to_string()),
    ];
    self
      .get_proto(format!("{SPCLIENT}/searchview/v3/search"), &q, "searchview")
      .await
  }

  async fn batch(&self, uris: &[String], kind: ExtensionKind) -> Result<HashMap<String, Vec<u8>>> {
    if uris.is_empty() {
      return Ok(HashMap::new());
    }
    let (country, catalogue) = self.http.market().await;
    let mut header = BatchedEntityRequestHeader::new();
    header.country = country;
    header.catalogue = catalogue;
    let mut req = BatchedEntityRequest::new();
    req.header = MessageField::some(header);
    for u in uris {
      let mut q = ExtensionQuery::new();
      q.extension_kind = kind.into();
      let mut er = EntityRequest::new();
      er.entity_uri = u.clone();
      er.query.push(q);
      req.entity_request.push(er);
    }
    let resp: BatchedExtensionResponse = self
      .post_proto(
        format!("{SPCLIENT}/extended-metadata/v0/extended-metadata"),
        req.write_to_bytes()?,
        "extended-metadata",
      )
      .await?;
    let mut out = HashMap::new();
    for arr in &resp.extended_metadata {
      for d in &arr.extension_data {
        let bytes = d.extension_data.value.clone();
        if !bytes.is_empty() {
          out.insert(d.entity_uri.clone(), bytes);
        }
      }
    }
    Ok(out)
  }

  pub async fn get_tracks(&self, ids: &[String]) -> Result<HashMap<String, Track>> {
    Ok(parse_map(
      self.batch(&normalize(ids, "track"), ExtensionKind::TRACK_V4).await?,
    ))
  }
  pub async fn get_artists(&self, ids: &[String]) -> Result<HashMap<String, Artist>> {
    Ok(parse_map(
      self.batch(&normalize(ids, "artist"), ExtensionKind::ARTIST_V4).await?,
    ))
  }
  pub async fn get_albums(&self, ids: &[String]) -> Result<HashMap<String, Album>> {
    Ok(parse_map(
      self.batch(&normalize(ids, "album"), ExtensionKind::ALBUM_V4).await?,
    ))
  }
  pub async fn get_shows(&self, ids: &[String]) -> Result<HashMap<String, Show>> {
    Ok(parse_map(
      self.batch(&normalize(ids, "show"), ExtensionKind::SHOW_V4).await?,
    ))
  }
  pub async fn get_episodes(&self, ids: &[String]) -> Result<HashMap<String, Episode>> {
    Ok(parse_map(
      self
        .batch(&normalize(ids, "episode"), ExtensionKind::EPISODE_V4)
        .await?,
    ))
  }

  pub async fn get_playlist(&self, playlist_id: &str, from: u32, length: Option<u32>) -> Result<SelectedListContent> {
    let mut q = Vec::new();
    if let Some(l) = length {
      q.push(("from", from.to_string()));
      q.push(("length", l.to_string()));
    }
    self
      .get_proto(format!("{SPCLIENT}/playlist/v2/playlist/{playlist_id}"), &q, "playlist")
      .await
  }

  // ---- user-scoped (need the canonical username) --------------------------

  pub async fn rootlist(&self, username: &str) -> Result<SelectedListContent> {
    let q = vec![
      ("decorate", "revision,attributes,length,owner,timestamp".to_string()),
      ("from", "0".to_string()),
      ("length", "500".to_string()),
    ];
    self
      .get_proto(
        format!("{SPCLIENT}/playlist/v2/user/{username}/rootlist"),
        &q,
        "rootlist",
      )
      .await
  }

  pub async fn recently_played(&self, username: &str, limit: u32) -> Result<RecentlyPlayed> {
    let q = vec![
      ("limit", limit.to_string()),
      ("filter", "default,collection-new-episodes".to_string()),
    ];
    self
      .get_proto(
        format!("{SPCLIENT}/recently-played/v3/user/{username}/recently-played"),
        &q,
        "recently-played",
      )
      .await
  }

  pub async fn collection_paging(&self, username: &str, set: &str, limit: u64) -> Result<Vec<CollectionItem>> {
    let body = util::collection_paging_body(username, set, limit);
    let mut headers = self.http.headers(false).await?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(COLLECTION_CT));
    headers.insert(ACCEPT, HeaderValue::from_static(COLLECTION_CT));
    let resp = self
      .http
      .send(
        HttpMethod::Post,
        format!("{SPCLIENT}/collection/v2/paging"),
        headers,
        body,
        0,
      )
      .await?;
    if !resp.ok() {
      return Err(Error::status("collection_paging", resp.status, resp.text()));
    }
    Ok(util::parse_collection_page(&resp.body))
  }

  pub async fn collection_write(&self, username: &str, set: &str, add: &[String], remove: &[String]) -> Result<u16> {
    let mut req = WriteRequest::new();
    req.username = username.to_string();
    req.set = set.to_string();
    let now = now_secs();
    for u in add {
      let mut it = CollectionWriteItem::new();
      it.uri = normalize(std::slice::from_ref(u), "track").remove(0);
      it.added_at = now;
      req.items.push(it);
    }
    for u in remove {
      let mut it = CollectionWriteItem::new();
      it.uri = normalize(std::slice::from_ref(u), "track").remove(0);
      it.is_removed = true;
      req.items.push(it);
    }
    let mut headers = self.http.headers(false).await?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(COLLECTION_CT));
    headers.insert(ACCEPT, HeaderValue::from_static(COLLECTION_CT));
    let resp = self
      .http
      .send(
        HttpMethod::Post,
        format!("{SPCLIENT}/collection/v2/write"),
        headers,
        req.write_to_bytes()?,
        0,
      )
      .await?;
    Ok(resp.status)
  }
}

fn parse_map<T: Message>(raw: HashMap<String, Vec<u8>>) -> HashMap<String, T> {
  raw
    .into_iter()
    .filter_map(|(k, v)| T::parse_from_bytes(&v).ok().map(|m| (k, m)))
    .collect()
}
