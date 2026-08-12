use std::{cell::RefCell, rc::Rc};

use bridgething_sdk_runtime::{Connector, InboundHalf, OutboundHalf, TransportError};
use futures::{
  SinkExt, StreamExt,
  channel::{mpsc, oneshot},
};
use libbridgething::{
  gateway::{BridgeToGatewayMsg, GatewayToBridgeMsg},
  protocol::{GatewayEndec, PrioritizedFrame},
};
use tokio_util::bytes::{Bytes, BytesMut};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::spawn_local;
use web_sys::{BinaryType, CloseEvent, Event, MessageEvent, WebSocket};

use crate::{
  GatewayProtocol,
  codec::{BATCH_BYTES, decode_step, encode_frame},
};

const OUTBOUND_CAP: usize = 32;

pub struct ChannelConnector {
  out: mpsc::Sender<Bytes>,
  inn: mpsc::UnboundedReceiver<Result<Bytes, TransportError>>,
}

pub struct LinkPorts {
  pub outbound: mpsc::Receiver<Bytes>,
  pub inbound: mpsc::UnboundedSender<Result<Bytes, TransportError>>,
}

pub struct ChannelOut {
  sink: mpsc::Sender<Bytes>,
}

pub struct ChannelIn {
  stream: mpsc::UnboundedReceiver<Result<Bytes, TransportError>>,
  decoder: GatewayEndec,
  buf: BytesMut,
}

pub fn channel_link() -> (ChannelConnector, LinkPorts) {
  let (out_tx, out_rx) = mpsc::channel(OUTBOUND_CAP);
  let (in_tx, in_rx) = mpsc::unbounded();
  (
    ChannelConnector {
      out: out_tx,
      inn: in_rx,
    },
    LinkPorts {
      outbound: out_rx,
      inbound: in_tx,
    },
  )
}

pub async fn connect_websocket(url: &str) -> Result<ChannelConnector, TransportError> {
  let (connector, ports) = channel_link();
  let (ready_tx, ready_rx) = oneshot::channel();
  let url = url.to_owned();

  spawn_local(async move { pump(url, ports.outbound, ports.inbound, ready_tx).await });

  ready_rx.await.map_err(|_| TransportError::Closed)??;
  Ok(connector)
}

impl Connector<GatewayProtocol> for ChannelConnector {
  type Out = ChannelOut;
  type In = ChannelIn;
  fn split(self) -> (ChannelOut, ChannelIn) {
    (
      ChannelOut { sink: self.out },
      ChannelIn {
        stream: self.inn,
        decoder: GatewayEndec::default(),
        buf: BytesMut::new(),
      },
    )
  }
}

impl OutboundHalf<GatewayProtocol> for ChannelOut {
  fn max_batch_bytes(&self) -> usize {
    BATCH_BYTES
  }

  fn encode(frame: PrioritizedFrame<GatewayToBridgeMsg>) -> Result<Bytes, TransportError> {
    encode_frame(frame)
  }

  async fn ready(&mut self) -> Result<(), TransportError> {
    futures::future::poll_fn(|cx| self.sink.poll_ready_unpin(cx))
      .await
      .map_err(|_| TransportError::Closed)
  }

  async fn send_batch(&mut self, batch: Bytes) -> Result<(), TransportError> {
    self.sink.send(batch).await.map_err(|_| TransportError::Closed)
  }
}

impl InboundHalf<GatewayProtocol> for ChannelIn {
  async fn recv(&mut self) -> Option<Result<BridgeToGatewayMsg, TransportError>> {
    loop {
      if let Some(result) = decode_step(&mut self.decoder, &mut self.buf) {
        return Some(result);
      }
      match self.stream.next().await {
        Some(Ok(bytes)) => self.buf.extend_from_slice(&bytes),
        Some(Err(err)) => return Some(Err(err)),
        None => return None,
      }
    }
  }
}

fn js_err(context: &str, value: &JsValue) -> TransportError {
  TransportError::Decode(format!("{context}: {value:?}"))
}

async fn pump(
  url: String,
  mut out_rx: mpsc::Receiver<Bytes>,
  in_tx: mpsc::UnboundedSender<Result<Bytes, TransportError>>,
  ready_tx: oneshot::Sender<Result<(), TransportError>>,
) {
  let socket = match WebSocket::new(&url) {
    Ok(socket) => socket,
    Err(err) => {
      let _ = ready_tx.send(Err(js_err("ws open", &err)));
      return;
    }
  };
  socket.set_binary_type(BinaryType::Arraybuffer);

  let (open_tx, open_rx) = oneshot::channel();
  let open_slot = Rc::new(RefCell::new(Some(open_tx)));
  let (closed_tx, closed_rx) = oneshot::channel();
  let closed_slot = Rc::new(RefCell::new(Some(closed_tx)));

  let on_message = {
    let in_tx = in_tx.clone();
    Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
      if let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
        let bytes = Bytes::from(js_sys::Uint8Array::new(&buffer).to_vec());
        let _ = in_tx.unbounded_send(Ok(bytes));
      }
    })
  };
  socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

  let on_open = {
    let open_slot = open_slot.clone();
    Closure::<dyn FnMut(Event)>::new(move |_| {
      if let Some(tx) = open_slot.borrow_mut().take() {
        let _ = tx.send(Ok(()));
      }
    })
  };
  socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));

  let on_error = {
    let open_slot = open_slot.clone();
    Closure::<dyn FnMut(Event)>::new(move |event: Event| {
      if let Some(tx) = open_slot.borrow_mut().take() {
        let _ = tx.send(Err(js_err("ws connect", event.as_ref())));
      }
    })
  };
  socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));

  let on_close = {
    let open_slot = open_slot.clone();
    let in_tx = in_tx.clone();
    Closure::<dyn FnMut(CloseEvent)>::new(move |_| {
      if let Some(tx) = open_slot.borrow_mut().take() {
        let _ = tx.send(Err(TransportError::Closed));
      }
      if let Some(tx) = closed_slot.borrow_mut().take() {
        let _ = tx.send(());
      }
      in_tx.close_channel();
    })
  };
  socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));

  let opened = match open_rx.await {
    Ok(result) => result,
    Err(_) => Err(TransportError::Closed),
  };
  let failed = opened.is_err();
  let _ = ready_tx.send(opened);

  if !failed {
    let mut closed = closed_rx;
    loop {
      futures::select! {
        _ = closed => break,
        next = out_rx.next() => match next {
          Some(bytes) => {
            if socket.send_with_u8_array(&bytes).is_err() {
              break;
            }
          }
          None => break,
        },
      }
    }
  }

  socket.set_onmessage(None);
  socket.set_onopen(None);
  socket.set_onerror(None);
  socket.set_onclose(None);
  let _ = socket.close();
  drop((on_message, on_open, on_error, on_close));
}
