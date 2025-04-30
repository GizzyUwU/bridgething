use std::{hash::Hasher, sync::Arc, time::Duration};

use ahash::AHasher;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bluer::{
  Address, AddressType,
  l2cap::{self, Socket, SocketAddr, Stream},
};
use libbridgething::{CARTHING_HACKS_LOGO, ServerEventType, server::ServerPlayerEvent};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  sync::mpsc,
  task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::{handler::client::MsgHandle, player::art::CoverArtCache, server::ClientMan};

// TODO: potentially make cover art synchronous? or more tied to player rather than bt?

type ArtTx = mpsc::Sender<(String, Option<MsgHandle>)>;
type ArtRx = mpsc::Receiver<(String, Option<MsgHandle>)>;

#[derive(Clone, Debug)]
pub struct CoverArt {
  client_man: ClientMan,
  cache: CoverArtCache,

  tx: ArtTx,
  _handle: Arc<JoinHandle<()>>,
}

impl CoverArt {
  pub fn init(client_man: ClientMan, cache: CoverArtCache, cancel_token: CancellationToken, address: Address) -> Self {
    let (tx, rx) = mpsc::channel(4);

    let inner = CoverArtInner {
      client_man: client_man.clone(),
      cache: cache.clone(),

      rx,
      cancel_token,

      address,

      last_image_hash: 0,
    };

    Self {
      client_man,
      cache,

      tx,
      _handle: Arc::new(inner.spawn()),
    }
  }

  /// this function will either send the image if it's in the cache or ask for it and send it later
  pub async fn fetch(&self, key: &String, handle: Option<MsgHandle>) {
    // handle the dummy image for stock firmware
    if key == "bridgething:image:bridgething:image" {
      send_image(
        &self.client_man,
        &handle,
        key.to_owned(),
        CARTHING_HACKS_LOGO.as_bytes(),
      )
      .await;
      return;
    }

    if let Some(image) = self.cache.get(key) {
      send_image(&self.client_man, &handle, key.to_owned(), &image).await;
    } else if let Err(err) = self.tx.send((key.to_owned(), handle)).await {
      tracing::error!("failed to send message to image fetch thread: {:?}", err);
    }
  }
}

const OBEX_HELLO: [u8; 26] = [
  0x80, 0x00, 0x1a, 0x15, 0x00, 0x06, 0x9b, 0x46, 0x00, 0x13, 0x71, 0x63, 0xdd, 0x54, 0x4a, 0x7e, 0x11, 0xe2, 0xb4,
  0x7c, 0x00, 0x50, 0xc2, 0x49, 0x00, 0x48,
];

#[derive(Debug)]
struct CoverArtInner {
  client_man: ClientMan,
  cache: CoverArtCache,

  rx: ArtRx,
  cancel_token: CancellationToken,

  address: Address,

  last_image_hash: u64,
}

impl CoverArtInner {
  pub fn spawn(mut self) -> JoinHandle<()> {
    tokio::spawn(async move { self.art_loop().await })
  }

  async fn art_loop(&mut self) {
    loop {
      tokio::select! {
        Some((image_id, handle)) = self.rx.recv() => self.maybe_get_image(image_id, handle).await,
        _ = self.cancel_token.cancelled() => break,
      }
    }

    tracing::debug!("cover art thread cancelled - shutting down");
  }

  async fn maybe_get_image(&mut self, image_id: String, handle: Option<MsgHandle>) {
    let mut retry_count = 0;

    loop {
      if retry_count > 5 {
        tracing::warn!("failed to get new cover art image after 5 tries");
        return;
      }

      match self.get_image(&image_id, &handle).await {
        Ok(success) => {
          if success {
            break;
          } else {
            tokio::time::sleep(Duration::from_secs(1)).await;
            retry_count += 1;
            continue;
          }
        }
        Err(err) => {
          tracing::error!("failed to get cover art image: {:?}", err);
          return;
        }
      }
    }
  }

  async fn create_stream(&mut self) -> bluer::Result<Stream> {
    tracing::debug!("connecting to obex channel for {:?}", self.address);
    let socket_addr = SocketAddr::new(self.address, AddressType::BrEdr, 0x1007);

    tracing::debug!("connecting to {:?}", &socket_addr);
    let socket = Socket::new_stream()?;
    socket.set_l2cap_opts(&create_l2cap_opts())?;
    let stream = socket.connect(socket_addr).await.expect("connection failed");
    tokio::time::sleep(Duration::from_millis(500)).await; // If this is removed, connection fails most of the time

    #[cfg(debug_assertions)]
    crate::bluetooth::debug::query_socket(&stream).await?;

    tracing::debug!("connected to obex channel for {:?}", self.address);
    Ok(stream)
  }

  async fn get_image(&mut self, image_id: &String, handle: &Option<MsgHandle>) -> bluer::Result<bool> {
    if let Some(image) = self.cache.get(image_id) {
      send_image(&self.client_man, handle, image_id.to_owned(), &image).await;
      return Ok(true);
    }

    let Ok(mut stream) = self.create_stream().await else {
      tracing::error!("failed to connect to obex port!!");
      return Ok(false);
    };

    tracing::debug!("attempting to get obex image from {:?}", self.address);

    stream.write_all(&OBEX_HELLO).await?;
    tracing::trace!("wrote obex hello");

    let mut buffer = vec![0; 1024];
    let n = stream.read(&mut buffer).await?;
    tracing::trace!("received {:?} bytes", n);
    let mut conn_id: [u8; 4] = [0, 0, 0, 0];

    if buffer[0] != 0xA0 {
      tracing::error!("obex handshake failed!");
      return Ok(false);
    } else {
      conn_id.copy_from_slice(&buffer[8..12]);
      tracing::debug!("obex connection id: {:?}", conn_id);
    }

    if conn_id == [0, 0, 0, 0] {
      tracing::error!("obex connection failed!!");
      return Ok(true);
    }

    if conn_id == [0, 0, 0, 1] {
      tracing::error!("this script does not support android!!");
      return Ok(true);
    }

    tracing::debug!("connected to obex channel for {:?} - getting new image", self.address);
    let image = get_ios_image(&mut stream, conn_id).await;

    let mut hasher = AHasher::default();
    hasher.write(&image);
    let hash = hasher.finish();

    if hash == self.last_image_hash {
      tracing::warn!("got same image as last time - not saving");
      return Ok(false);
    }
    self.last_image_hash = hash;

    send_image(&self.client_man, handle, image_id.to_owned(), &image).await;

    self.cache.insert(image_id.to_owned(), image);
    tracing::debug!("successfully got obex image!");
    Ok(true)
  }
}

async fn send_image(client_man: &ClientMan, handle: &Option<MsgHandle>, image_id: String, image: &[u8]) {
  let data = STANDARD.encode(image);
  if let Some(handle) = handle {
    if let Err(err) = handle
      .respond(ServerPlayerEvent::Image {
        id: image_id,
        height: 200,
        width: 200,
        data,
      })
      .await
    {
      tracing::error!("failed to broadcast image: {:?}", err);
    }
  } else if let Err(err) = client_man
    .broadcast(
      ServerPlayerEvent::Image {
        id: image_id,
        height: 200,
        width: 200,
        data,
      },
      ServerEventType::Event,
    )
    .await
  {
    tracing::error!("failed to broadcast image: {:?}", err);
  }
}

// TODO: make this safe for fuck's sake
async fn get_ios_image(stream: &mut Stream, conn_id: [u8; 4]) -> Vec<u8> {
  let handle_str = format!("{:07}", 0);
  let mut req = vec![0x83, 0x00, 0x2d, 0xcb];
  req.extend_from_slice(&conn_id);
  req.extend_from_slice(&[
    0x42, 0x00, 0x10, 0x78, 0x2d, 0x62, 0x74, 0x2f, 0x69, 0x6d, 0x67, 0x2d, 0x74, 0x68, 0x6d, 0x00, 0x30, 0x00, 0x13,
    0x00,
  ]);
  for ch in handle_str.chars() {
    req.push(ch as u8);
    req.push(0);
  }
  req.extend_from_slice(&[0x00, 0x97, 0x01]);

  let mut buffer = vec![];

  loop {
    stream.write_all(&req).await.unwrap();
    let mut buf = [0u8; 1025];
    let n = stream.read(&mut buf).await.unwrap();

    if n == 0 {
      break;
    }

    match buf[0] {
      0x90 => buffer.write_all(&buf[6..n]).await.unwrap(),
      0xA0 => {
        buffer.write_all(&buf[6..n]).await.unwrap();
        break;
      }
      _ => {
        tracing::error!("unknown obex error");
        break;
      }
    };
  }

  tracing::debug!("successfully got cover art image with length {:?}", buffer.len());
  buffer
}

fn create_l2cap_opts() -> l2cap::Opts {
  let mut opts = l2cap::Opts::default();
  opts.omtu = 672;
  opts.imtu = 1024;
  opts.flush_to = 65535;
  opts.mode = 3;
  opts.fcs = 1;
  opts.max_tx = 16;
  opts.txwin_size = 63;

  opts
}
