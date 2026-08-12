use std::{
  collections::BTreeMap,
  sync::{Arc, RwLock},
  time::Duration,
};

use bridgething_gateway::{HandlerError, OutboundLink, OutboundLinkExt, Reply};
use libbridgething::{
  RangePart,
  gateway::{
    GatewayToBridgeTransferMsgEvent, OtaAssetRange, OtaAssetRangeRejected, OtaAssetRangeReply, TransferAbandon,
    TransferBody, TransferRef,
  },
};
use uuid::Uuid;

use crate::{
  ota::stream::{Artifact, ArtifactStreamer},
  seam::Clock,
  transfer::{AckWindow, SourceRange},
};

pub const INLINE_RANGE_MAX_BYTES: u32 = 16 * 1024;
pub const RANGE_ACK_TIMEOUT_MS: u64 = 30_000;

pub struct RangeServer {
  link: Arc<dyn OutboundLink>,
  streamer: ArtifactStreamer,
  assets: RwLock<BTreeMap<String, Arc<dyn Artifact>>>,
}

impl RangeServer {
  pub fn new(link: Arc<dyn OutboundLink>, acks: Arc<AckWindow>, clock: Arc<dyn Clock>) -> Arc<Self> {
    Arc::new(Self {
      streamer: ArtifactStreamer::new(link.clone(), acks, clock),
      link,
      assets: RwLock::new(BTreeMap::new()),
    })
  }

  pub fn set_assets(&self, assets: BTreeMap<String, Arc<dyn Artifact>>) {
    *self.assets.write().unwrap() = assets;
  }

  pub fn assets(&self) -> BTreeMap<String, Arc<dyn Artifact>> {
    self.assets.read().unwrap().clone()
  }

  pub fn acks(&self) -> &Arc<AckWindow> {
    self.streamer.acks()
  }

  pub async fn answer(
    self: &Arc<Self>,
    id: Uuid,
    request: OtaAssetRange,
  ) -> Result<Reply<OtaAssetRangeReply>, HandlerError<OtaAssetRangeRejected>> {
    let asset = self.assets.read().unwrap().get(&request.asset).cloned();
    let Some(artifact) = asset else {
      return Err(reject(format!(
        "companion has no cached .zck for asset {}",
        request.asset
      )));
    };

    let total_size = match artifact.size() {
      Ok(size) => match u32::try_from(size) {
        Ok(size) => size,
        Err(_) => return Err(reject("zck size unavailable or > 4 GiB".into())),
      },
      Err(e) => return Err(reject(format!("sizing the zck failed: {e}"))),
    };

    for range in &request.ranges {
      if range.start.checked_add(range.length).is_none_or(|end| end > total_size) {
        return Err(reject(format!(
          "range {}+{} exceeds zck size {total_size}",
          range.start, range.length
        )));
      }
    }

    let parts: Vec<RangePart> = request
      .ranges
      .iter()
      .map(|range| RangePart {
        start: range.start,
        length: range.length,
      })
      .collect();
    let Ok(stream_len) = u32::try_from(parts.iter().map(|part| u64::from(part.length)).sum::<u64>()) else {
      return Err(reject("requested ranges total more than 4 GiB".into()));
    };

    if stream_len <= INLINE_RANGE_MAX_BYTES {
      return self.answer_inline(artifact.as_ref(), total_size, parts, stream_len);
    }

    let reply = OtaAssetRangeReply {
      total_size,
      parts: parts.clone(),
      body: TransferBody::Stream(TransferRef {
        id,
        total_size: stream_len,
        sha256: None,
      }),
    };

    let ranges: Vec<SourceRange> = parts
      .iter()
      .map(|part| SourceRange {
        start: u64::from(part.start),
        length: u64::from(part.length),
      })
      .collect();
    let server = self.clone();
    Ok(Reply::new(reply).then(async move { server.stream(id, artifact, request.asset, ranges).await }))
  }

  async fn stream(&self, id: Uuid, artifact: Arc<dyn Artifact>, asset: String, ranges: Vec<SourceRange>) {
    let pushed = self
      .streamer
      .stream(
        id,
        artifact.as_ref(),
        &asset,
        &ranges,
        0,
        Duration::from_millis(RANGE_ACK_TIMEOUT_MS),
        &|_| {},
      )
      .await;

    if let Err(e) = pushed {
      let _ = self
        .link
        .event(GatewayToBridgeTransferMsgEvent::Abandon(TransferAbandon {
          transfer_id: id,
          reason: format!("range stream failed: {e}"),
        }))
        .await;
    }
    self.streamer.acks().finish(id);
  }

  fn answer_inline(
    &self,
    source: &dyn Artifact,
    total_size: u32,
    parts: Vec<RangePart>,
    stream_len: u32,
  ) -> Result<Reply<OtaAssetRangeReply>, HandlerError<OtaAssetRangeRejected>> {
    let mut body = vec![0u8; stream_len as usize];
    let mut at = 0usize;
    for part in &parts {
      let end = at + part.length as usize;
      match source.read_at(u64::from(part.start), &mut body[at..end]) {
        Ok(read) if read == part.length as usize => at = end,
        Ok(_) => return Err(reject("short read from zck".into())),
        Err(_) => return Err(reject("read zck failed".into())),
      }
    }

    Ok(Reply::new(OtaAssetRangeReply {
      total_size,
      parts,
      body: TransferBody::Inline(body),
    }))
  }
}

fn reject(reason: String) -> HandlerError<OtaAssetRangeRejected> {
  HandlerError::Domain(OtaAssetRangeRejected { reason })
}

#[cfg(test)]
mod tests {
  use std::{collections::BTreeMap, sync::Arc, time::Duration};

  use libbridgething::{RangeSpec, gateway::TransferBody};

  use super::{INLINE_RANGE_MAX_BYTES, RANGE_ACK_TIMEOUT_MS, RangeServer};
  use crate::{
    ota::{
      harness::{FakeDevice, Spool, TestClock, linked_gateway, pattern, route_ranges},
      service::{BOOT_ZCK_ASSET, SYSTEM_ZCK_ASSET},
    },
    transfer::AckWindow,
  };

  struct Rig {
    spool: Spool,
    device: FakeDevice,
    server: Arc<RangeServer>,
  }

  fn rig() -> Rig {
    let (gateway, device) = linked_gateway();
    let server = RangeServer::new(Arc::new(gateway.clone()), Arc::new(AckWindow::new()), TestClock::new());
    route_ranges(&gateway, &server);
    Rig {
      spool: Spool::new(),
      device,
      server,
    }
  }

  fn filled(byte: u8, len: usize) -> Vec<u8> {
    vec![byte; len]
  }

  #[test]
  fn the_inline_cap_is_the_shipped_number() {
    assert_eq!(INLINE_RANGE_MAX_BYTES, 16 * 1024);
    assert_eq!(RANGE_ACK_TIMEOUT_MS, 30_000);
  }

  #[tokio::test]
  async fn a_range_routes_to_the_asset_it_names() {
    let mut rig = rig();
    let system = rig.spool.asset("system.zck", &filled(0xAA, 256));
    let boot = rig.spool.asset("boot.zck", &filled(0xBB, 256));
    rig.server.set_assets(BTreeMap::from([
      (SYSTEM_ZCK_ASSET.to_string(), system),
      (BOOT_ZCK_ASSET.to_string(), boot),
    ]));

    let boot_id = rig
      .device
      .ask_range(BOOT_ZCK_ASSET, vec![RangeSpec { start: 0, length: 256 }]);
    let boot_reply = rig
      .device
      .await_range_reply(boot_id)
      .await
      .expect("the boot asset is served");
    let TransferBody::Inline(boot_body) = boot_reply.body else {
      panic!("a 256 byte range answers inline");
    };
    assert_eq!(boot_body, filled(0xBB, 256), "the boot asset comes from the boot zck");

    let system_id = rig
      .device
      .ask_range(SYSTEM_ZCK_ASSET, vec![RangeSpec { start: 0, length: 256 }]);
    let system_reply = rig
      .device
      .await_range_reply(system_id)
      .await
      .expect("the system asset is served");
    let TransferBody::Inline(system_body) = system_reply.body else {
      panic!("a 256 byte range answers inline");
    };
    assert_eq!(
      system_body,
      filled(0xAA, 256),
      "the system asset comes from the system zck"
    );
  }

  #[tokio::test]
  async fn an_unknown_asset_is_rejected_by_name() {
    let mut rig = rig();
    rig.server.set_assets(BTreeMap::new());

    let id = rig
      .device
      .ask_range("does-not-exist.zck", vec![RangeSpec { start: 0, length: 16 }]);
    let reason = rig
      .device
      .await_range_reply(id)
      .await
      .expect_err("an asset the companion never cached is rejected");

    assert!(
      reason.contains("does-not-exist.zck"),
      "the rejection must name the missing asset, got {reason}"
    );
  }

  #[tokio::test]
  async fn a_range_past_the_end_is_rejected_with_the_size() {
    let mut rig = rig();
    let key = rig.spool.asset("system.zck", &pattern(256));
    rig
      .server
      .set_assets(BTreeMap::from([(SYSTEM_ZCK_ASSET.to_string(), key)]));

    let id = rig.device.ask_range(
      SYSTEM_ZCK_ASSET,
      vec![RangeSpec {
        start: 128,
        length: 256,
      }],
    );
    let reason = rig
      .device
      .await_range_reply(id)
      .await
      .expect_err("an over-long range fails");

    assert!(
      reason.contains("256"),
      "the rejection must name the zck size, got {reason}"
    );
  }

  #[tokio::test]
  async fn a_range_that_would_wrap_is_rejected_rather_than_served() {
    let mut rig = rig();
    let key = rig.spool.asset("system.zck", &pattern(256));
    rig
      .server
      .set_assets(BTreeMap::from([(SYSTEM_ZCK_ASSET.to_string(), key)]));

    let id = rig.device.ask_range(
      SYSTEM_ZCK_ASSET,
      vec![RangeSpec {
        start: u32::MAX - 8,
        length: 64,
      }],
    );
    let reason = rig
      .device
      .await_range_reply(id)
      .await
      .expect_err("an overflowing range is not a range");

    assert!(!reason.is_empty());
  }

  #[tokio::test]
  async fn one_bad_range_rejects_the_whole_request() {
    let mut rig = rig();
    let key = rig.spool.asset("system.zck", &pattern(256));
    rig
      .server
      .set_assets(BTreeMap::from([(SYSTEM_ZCK_ASSET.to_string(), key)]));

    let id = rig.device.ask_range(
      SYSTEM_ZCK_ASSET,
      vec![
        RangeSpec { start: 0, length: 64 },
        RangeSpec {
          start: 200,
          length: 400,
        },
      ],
    );

    assert!(
      rig.device.await_range_reply(id).await.is_err(),
      "a partially servable request is not partially served"
    );
  }

  #[tokio::test]
  async fn several_parts_come_back_concatenated_in_request_order() {
    let mut rig = rig();
    let body = pattern(1_024);
    let key = rig.spool.asset("system.zck", &body);
    rig
      .server
      .set_assets(BTreeMap::from([(SYSTEM_ZCK_ASSET.to_string(), key)]));

    let id = rig.device.ask_range(
      SYSTEM_ZCK_ASSET,
      vec![RangeSpec { start: 512, length: 64 }, RangeSpec { start: 0, length: 32 }],
    );
    let reply = rig.device.await_range_reply(id).await.expect("both parts are served");

    assert_eq!(reply.total_size, 1_024, "total_size is the asset, not the range");
    assert_eq!(reply.parts.len(), 2);
    assert_eq!((reply.parts[0].start, reply.parts[0].length), (512, 64));
    assert_eq!((reply.parts[1].start, reply.parts[1].length), (0, 32));
    let TransferBody::Inline(inline) = reply.body else {
      panic!("96 bytes answers inline");
    };
    let mut want = body[512..576].to_vec();
    want.extend_from_slice(&body[0..32]);
    assert_eq!(inline, want, "parts concatenate in the order they were asked for");
  }

  #[tokio::test]
  async fn a_range_at_the_inline_cap_still_answers_inline() {
    let mut rig = rig();
    let body = pattern(INLINE_RANGE_MAX_BYTES as usize * 2);
    let key = rig.spool.asset("system.zck", &body);
    rig
      .server
      .set_assets(BTreeMap::from([(SYSTEM_ZCK_ASSET.to_string(), key)]));

    let id = rig.device.ask_range(
      SYSTEM_ZCK_ASSET,
      vec![RangeSpec {
        start: 0,
        length: INLINE_RANGE_MAX_BYTES,
      }],
    );
    let reply = rig.device.await_range_reply(id).await.expect("served");

    assert!(
      matches!(reply.body, TransferBody::Inline(_)),
      "the cap is inclusive, so exactly 16 KiB does not stream"
    );
  }

  #[tokio::test]
  async fn a_range_over_the_inline_cap_streams_against_acks() {
    let mut rig = rig();
    let size = 256 * 1024;
    let body = pattern(size);
    let key = rig.spool.asset("system.zck", &body);
    rig
      .server
      .set_assets(BTreeMap::from([(SYSTEM_ZCK_ASSET.to_string(), key)]));

    let id = rig.device.ask_range(
      SYSTEM_ZCK_ASSET,
      vec![RangeSpec {
        start: 0,
        length: size as u32,
      }],
    );
    let reply = rig.device.await_range_reply(id).await.expect("served");
    let TransferBody::Stream(reference) = reply.body else {
      panic!("a range over the inline cap must stream");
    };
    assert_eq!(
      reference.total_size as usize, size,
      "the ref sizes the stream, not the asset"
    );

    let mut assembled: Vec<u8> = Vec::new();
    let first = rig.device.next_fragment(reference.id).await;
    assert_eq!(first.offset, 0, "a streamed range starts its own offsets at zero");
    assembled.extend_from_slice(&first.bytes);

    while assembled.len() < size {
      rig.device.ack(reference.id, assembled.len() as u32);
      let fragment = rig.device.next_fragment(reference.id).await;
      assert_eq!(
        fragment.offset as usize,
        assembled.len(),
        "range fragments arrive contiguous in stream order"
      );
      assembled.extend_from_slice(&fragment.bytes);
    }

    assert_eq!(assembled, body, "the streamed bytes are the range that was asked for");
  }

  #[tokio::test]
  async fn a_multi_part_stream_numbers_its_offsets_across_the_parts() {
    let mut rig = rig();
    let part = 24 * 1024;
    let body = pattern(part * 4);
    let key = rig.spool.asset("system.zck", &body);
    rig
      .server
      .set_assets(BTreeMap::from([(SYSTEM_ZCK_ASSET.to_string(), key)]));

    let id = rig.device.ask_range(
      SYSTEM_ZCK_ASSET,
      vec![
        RangeSpec {
          start: (part * 2) as u32,
          length: part as u32,
        },
        RangeSpec {
          start: 0,
          length: part as u32,
        },
      ],
    );
    let reply = rig.device.await_range_reply(id).await.expect("served");
    let TransferBody::Stream(reference) = reply.body else {
      panic!("48 KiB must stream");
    };

    let mut assembled: Vec<u8> = Vec::new();
    while assembled.len() < part * 2 {
      let fragment = rig.device.next_fragment(reference.id).await;
      assert_eq!(
        fragment.offset as usize,
        assembled.len(),
        "one offset space across all parts"
      );
      assembled.extend_from_slice(&fragment.bytes);
      rig.device.ack(reference.id, assembled.len() as u32);
    }

    let mut want = body[part * 2..part * 3].to_vec();
    want.extend_from_slice(&body[0..part]);
    assert_eq!(assembled, want);
  }

  #[tokio::test(start_paused = true)]
  async fn a_range_stream_the_daemon_stops_acking_is_abandoned() {
    let mut rig = rig();
    let size = 512 * 1024;
    let key = rig.spool.asset("system.zck", &pattern(size));
    rig
      .server
      .set_assets(BTreeMap::from([(SYSTEM_ZCK_ASSET.to_string(), key)]));

    let id = rig.device.ask_range(
      SYSTEM_ZCK_ASSET,
      vec![RangeSpec {
        start: 0,
        length: size as u32,
      }],
    );
    let reply = rig.device.await_range_reply(id).await.expect("served");
    let TransferBody::Stream(reference) = reply.body else {
      panic!("half a megabyte must stream");
    };
    rig.device.next_fragment(reference.id).await;

    let abandon = rig
      .device
      .await_abandon_within(Duration::from_millis(RANGE_ACK_TIMEOUT_MS * 2), reference.id)
      .await;

    assert!(
      abandon.reason.contains("range stream failed"),
      "an abandoned range says so, got {}",
      abandon.reason
    );
  }

  #[tokio::test]
  async fn a_zero_length_range_is_answered_rather_than_streamed() {
    let mut rig = rig();
    let key = rig.spool.asset("system.zck", &pattern(256));
    rig
      .server
      .set_assets(BTreeMap::from([(SYSTEM_ZCK_ASSET.to_string(), key)]));

    let id = rig
      .device
      .ask_range(SYSTEM_ZCK_ASSET, vec![RangeSpec { start: 0, length: 0 }]);
    let reply = rig.device.await_range_reply(id).await.expect("an empty range is legal");

    let TransferBody::Inline(inline) = reply.body else {
      panic!("nothing to send is not a stream");
    };
    assert!(inline.is_empty());
  }
}
