#![cfg(feature = "emulator")]

mod emu;

use std::time::Duration;

use bridgething_iap2::{
  EmulatorEvent, SessionEvent,
  csm::now_playing::{MediaItemAttributes, NowPlayingUpdate},
};
use bytes::Bytes;
use emu::recv_with_timeout;

#[tokio::test]
async fn emulator_handle_drives_now_playing_and_artwork() {
  let artwork = Bytes::from(vec![0x5Eu8; 5000]);
  let (mut harness, mut emu_events, handle) = emu::spawn(emu::identification_config(), None, |emulator| {
    emulator.without_now_playing()
  });

  loop {
    match recv_with_timeout(&mut emu_events, Duration::from_secs(10)).await {
      Some(EmulatorEvent::Identified) => break,
      Some(_) => continue,
      None => panic!("emulator exited before identification"),
    }
  }

  handle
    .push_now_playing(NowPlayingUpdate {
      media_item: Some(MediaItemAttributes {
        persistent_id: Some(0xBEEF),
        title: Some("Driven Track".into()),
        artwork_id: Some(200),
        ..Default::default()
      }),
      playback: None,
    })
    .await
    .expect("drive now-playing");
  handle.push_artwork(200, artwork.clone()).await.expect("drive artwork");

  let mut saw_now_playing = false;
  loop {
    let evt = recv_with_timeout(&mut harness.acc_events, Duration::from_secs(10))
      .await
      .expect("accessory event timed out before driven artwork");
    match evt {
      SessionEvent::NowPlayingUpdate(update) => {
        let media = update.media_item.expect("media_item group present");
        if media.title.as_deref() == Some("Driven Track") {
          assert_eq!(media.artwork_id, Some(200), "driven art id pairs with the transfer id");
          saw_now_playing = true;
        }
      }
      SessionEvent::ArtworkBytes { transfer_id, bytes } => {
        assert!(
          saw_now_playing,
          "driven NowPlaying delta must precede the artwork bytes"
        );
        assert_eq!(transfer_id, 200);
        assert_eq!(bytes.len(), artwork.len());
        assert!(
          bytes.iter().all(|&b| b == 0x5E),
          "driven artwork bytes round-trip intact"
        );
        return;
      }
      _ => continue,
    }
  }
}
