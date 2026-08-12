use std::{sync::Arc, time::Duration};

use bridgething_companion::{
  dispatch::asset::{ASSET_FRAGMENT_BYTES, AssetDispatcher, INLINE_BODY_MAX_BYTES},
  provider::{AssetBytes, ProviderError},
};
use bridgething_delivery::seam::{Clock, SystemClock};
use bridgething_gateway::{AssetHandler, HandlerError, route};
use libbridgething::{
  gateway::{
    AssetGotReply, AssetRequest, BridgeToGatewayAssetMsg, BridgeToGatewayMsg, BridgeToGatewayMsgData,
    GatewayToBridgeAssetMsg, GatewayToBridgeMsg, GatewayToBridgeMsgData, GatewayToBridgeTransferMsg, TransferBody,
    TransferFragment,
  },
  wire::MsgMeta,
};
use uuid::Uuid;

use crate::{
  fakes::{FakeProvider, FakeRegistry},
  routing::Routed,
  support::Peer,
};

impl Peer {
  fn first_at<T>(&self, pick: impl Fn(&GatewayToBridgeMsg) -> Option<T>) -> Option<usize> {
    self.seen.lock().unwrap().iter().position(|msg| pick(msg).is_some())
  }
}

fn clock() -> Arc<dyn Clock> {
  Arc::new(SystemClock)
}

fn got(msg: &GatewayToBridgeMsg) -> Option<AssetGotReply> {
  match &msg.data {
    GatewayToBridgeMsgData::Asset(GatewayToBridgeAssetMsg::Got(reply)) => Some(reply.clone()),
    _ => None,
  }
}

fn asset_request(id: &str) -> BridgeToGatewayMsg {
  BridgeToGatewayMsg {
    id: Uuid::now_v7(),
    meta: MsgMeta::Request,
    data: BridgeToGatewayMsgData::Asset(BridgeToGatewayAssetMsg::Request(AssetRequest {
      id: id.to_owned(),
      request_id: Uuid::now_v7(),
    })),
  }
}

fn fragment(msg: &GatewayToBridgeMsg) -> Option<TransferFragment> {
  match &msg.data {
    GatewayToBridgeMsgData::Transfer(GatewayToBridgeTransferMsg::Fragment(fragment)) => Some(fragment.clone()),
    _ => None,
  }
}

#[tokio::test]
async fn an_asset_under_the_inline_cap_rides_inline() {
  let payload = vec![0x89, 0x50, 0x4e, 0x47];
  let carried = payload.clone();
  let provider = Arc::new(FakeProvider {
    on_asset: Some(Box::new(move |id| {
      assert_eq!(id, "art:track:1");
      Ok(Some(AssetBytes {
        bytes: carried.clone(),
        mime: Some("image/png".into()),
      }))
    })),
    ..FakeProvider::named("art")
  });
  let (gateway, _peer) = Peer::link();
  let dispatch = AssetDispatcher::new(FakeRegistry::with(provider.clone()), Arc::new(gateway), clock());

  let reply = dispatch
    .request(AssetRequest {
      id: "art:track:1".into(),
      request_id: Uuid::now_v7(),
    })
    .await
    .expect("the asset resolved");

  assert_eq!(reply.response.id, "art:track:1");
  assert_eq!(reply.response.mime.as_deref(), Some("image/png"));
  assert_eq!(reply.response.body, TransferBody::Inline(payload));
  assert!(reply.after.is_none(), "an inline body has nothing to push behind it");
  assert!(provider.saw("asset:art:track:1"));
}

#[tokio::test]
async fn an_asset_no_provider_has_answers_not_found() {
  let (gateway, _peer) = Peer::link();
  let dispatch = AssetDispatcher::new(
    FakeRegistry::with(FakeProvider::bare("art")),
    Arc::new(gateway),
    clock(),
  );

  let refused = dispatch
    .request(AssetRequest {
      id: "art:missing".into(),
      request_id: Uuid::now_v7(),
    })
    .await
    .expect_err("a miss is a refusal");

  match refused {
    HandlerError::Domain(reply) => assert_eq!(reply.id, "art:missing"),
    other => panic!("a miss is the domain error, not {other:?}"),
  }
}

#[tokio::test]
async fn a_provider_failure_answers_not_found_rather_than_leaving_the_request_open() {
  let provider = Arc::new(FakeProvider {
    on_asset: Some(Box::new(|_| Err(ProviderError::Failed("token expired".into())))),
    ..FakeProvider::named("art")
  });
  let (gateway, _peer) = Peer::link();
  let dispatch = AssetDispatcher::new(FakeRegistry::with(provider), Arc::new(gateway), clock());

  let refused = dispatch
    .request(AssetRequest {
      id: "art:boom".into(),
      request_id: Uuid::now_v7(),
    })
    .await
    .expect_err("a failed resolve is a refusal");
  assert!(matches!(refused, HandlerError::Domain(_)));
}

#[tokio::test]
async fn an_asset_over_the_inline_cap_streams_only_once_its_reply_is_on_the_wire() {
  let payload: Vec<u8> = (0..40 * 1024).map(|at| (at % 251) as u8).collect();
  let carried = payload.clone();
  let provider = Arc::new(FakeProvider {
    on_asset: Some(Box::new(move |_| {
      Ok(Some(AssetBytes {
        bytes: carried.clone(),
        mime: Some("image/jpeg".into()),
      }))
    })),
    ..FakeProvider::named("art")
  });
  let (gateway, peer) = Peer::link();
  let dispatch = AssetDispatcher::new(FakeRegistry::with(provider), Arc::new(gateway), clock());

  let request_id = Uuid::now_v7();
  let reply = dispatch
    .request(AssetRequest {
      id: "art:big".into(),
      request_id,
    })
    .await
    .expect("the asset resolved");

  let TransferBody::Stream(reference) = reply.response.body else {
    panic!("an asset over {INLINE_BODY_MAX_BYTES} bytes must declare a stream")
  };
  assert_eq!(reference.id, request_id, "the stream is keyed by the request id");
  assert_eq!(reference.total_size, payload.len() as u32);

  peer
    .quiet("an asset fragment", |msg| fragment(msg).map(|f| f.offset))
    .await;

  let pushing = tokio::spawn(reply.after.expect("a streamed reply carries its push"));

  let mut assembled = 0usize;
  while assembled < payload.len() {
    let at = assembled as u32;
    let found = peer
      .wait("an asset fragment", move |msg| {
        fragment(msg).filter(|f| f.offset == at && f.transfer_id == request_id)
      })
      .await;
    assert!(
      found.bytes.len() <= ASSET_FRAGMENT_BYTES,
      "an asset fragments at {ASSET_FRAGMENT_BYTES} bytes, got {}",
      found.bytes.len()
    );
    assert_eq!(
      payload[assembled..assembled + found.bytes.len()],
      found.bytes[..],
      "the fragment at {assembled} carries the payload's bytes"
    );
    assembled += found.bytes.len();
    dispatch.acks().note(request_id, assembled as u64);
  }
  assert_eq!(assembled, payload.len());
  pushing.await.expect("the push finished");
}

const SLOW_RESOLVE: Duration = Duration::from_millis(400);

#[tokio::test]
async fn six_asset_requests_through_route_overlap_rather_than_queueing_behind_each_other() {
  let provider = Arc::new(FakeProvider {
    delay: Some(SLOW_RESOLVE),
    on_asset: Some(Box::new(|_| {
      Ok(Some(AssetBytes {
        bytes: vec![1],
        mime: Some("image/jpeg".into()),
      }))
    })),
    ..FakeProvider::named("art")
  });
  let (gateway, peer) = Peer::link();
  let handlers = Routed::new(AssetDispatcher::new(
    FakeRegistry::with(provider),
    Arc::new(gateway.clone()),
    clock(),
  ));

  let started = std::time::Instant::now();
  for at in 0..6 {
    route(&handlers, asset_request(&format!("art:{at}")), gateway.connection())
      .await
      .expect("the routing path took the request");
  }
  let routed = started.elapsed();
  assert!(
    routed < SLOW_RESOLVE,
    "route must hand a concurrent request off rather than await it, took {routed:?}"
  );

  let waited = peer.wait_for("asset replies", 6, got).await;
  let total = routed + waited;
  assert!(
    total < SLOW_RESOLVE * 3,
    "six {SLOW_RESOLVE:?} resolves must overlap, six replies took {total:?}"
  );
}

#[tokio::test]
async fn the_router_enqueues_the_reply_before_it_runs_the_push_behind_it() {
  let payload: Vec<u8> = (0..40 * 1024).map(|at| (at % 251) as u8).collect();
  let carried = payload.clone();
  let provider = Arc::new(FakeProvider {
    on_asset: Some(Box::new(move |_| {
      Ok(Some(AssetBytes {
        bytes: carried.clone(),
        mime: Some("image/jpeg".into()),
      }))
    })),
    ..FakeProvider::named("art")
  });
  let (gateway, peer) = Peer::link();
  let handlers = Routed::new(AssetDispatcher::new(
    FakeRegistry::with(provider),
    Arc::new(gateway.clone()),
    clock(),
  ));

  let request = asset_request("art:ordered");
  let BridgeToGatewayMsgData::Asset(BridgeToGatewayAssetMsg::Request(ref inner)) = request.data else {
    panic!("the fixture is an asset request")
  };
  let transfer_id = inner.request_id;
  route(&handlers, request, gateway.connection())
    .await
    .expect("the routing path took the request");

  let acked = handlers.asset.acks().clone();
  tokio::spawn(async move {
    let mut at = 0u64;
    while at < 40 * 1024 {
      at += ASSET_FRAGMENT_BYTES as u64;
      acked.note(transfer_id, at.min(40 * 1024));
      tokio::time::sleep(Duration::from_millis(2)).await;
    }
  });

  peer.wait("the asset reply", got).await;
  peer
    .wait("an asset fragment", |msg| fragment(msg).map(|f| f.offset))
    .await;
  assert!(
    peer.first_at(got) < peer.first_at(|msg| fragment(msg).map(|f| f.offset)),
    "the ref has to reach the device before the fragments it announces"
  );
}

#[tokio::test]
async fn an_asset_id_prefixed_with_a_providers_name_goes_to_that_provider_first() {
  let owner = Arc::new(FakeProvider {
    on_asset: Some(Box::new(|_| {
      Ok(Some(AssetBytes {
        bytes: vec![7],
        mime: None,
      }))
    })),
    ..FakeProvider::named("media")
  });
  let other = FakeProvider::bare("spotify");
  let (gateway, _peer) = Peer::link();
  let dispatch = AssetDispatcher::new(
    FakeRegistry::of(vec![other.clone(), owner.clone()]),
    Arc::new(gateway),
    clock(),
  );

  let reply = dispatch
    .request(AssetRequest {
      id: "media/session/1".into(),
      request_id: Uuid::now_v7(),
    })
    .await
    .expect("the owning provider answered");

  assert_eq!(reply.response.body, TransferBody::Inline(vec![7]));
  assert!(owner.saw("asset:media/session/1"));
  assert!(
    !other.saw("asset:media/session/1"),
    "an id claimed by its owner must not be broadcast to every provider"
  );
}

#[tokio::test]
async fn an_unclaimed_asset_id_is_offered_to_every_provider_before_it_is_a_miss() {
  let first = FakeProvider::bare("spotify");
  let second = Arc::new(FakeProvider {
    on_asset: Some(Box::new(|_| {
      Ok(Some(AssetBytes {
        bytes: vec![3],
        mime: None,
      }))
    })),
    ..FakeProvider::named("applemusic")
  });
  let (gateway, _peer) = Peer::link();
  let dispatch = AssetDispatcher::new(
    FakeRegistry::of(vec![first.clone(), second.clone()]),
    Arc::new(gateway),
    clock(),
  );

  let reply = dispatch
    .request(AssetRequest {
      id: "art:unclaimed".into(),
      request_id: Uuid::now_v7(),
    })
    .await
    .expect("some provider had it");

  assert_eq!(reply.response.body, TransferBody::Inline(vec![3]));
  assert!(first.saw("asset:art:unclaimed"));
  assert!(second.saw("asset:art:unclaimed"));
}
