use std::time::Duration;

use bluer::{
  AddressType,
  l2cap::{Opts, Socket, SocketAddr, Stream},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const OBEX_HELLO: [u8; 26] = [
  0x80, 0x00, 0x1a, 0x15, 0x00, 0x06, 0x9b, 0x46, 0x00, 0x13, 0x71, 0x63, 0xdd, 0x54, 0x4a, 0x7e, 0x11, 0xe2, 0xb4,
  0x7c, 0x00, 0x50, 0xc2, 0x49, 0x00, 0x48,
];

fn create_l2cap_opts() -> Opts {
  let mut opts = Opts::default();
  opts.omtu = 672;
  opts.imtu = 1024;
  opts.flush_to = 65535;
  opts.mode = 3;
  opts.fcs = 1;
  opts.max_tx = 16;
  opts.txwin_size = 63;

  opts
}

#[tokio::main]
async fn main() -> bluer::Result<()> {
  let session = bluer::Session::new().await?;
  let adapter = session.default_adapter().await?;
  adapter.set_powered(true).await?;
  let target_sa = SocketAddr::new("00:00:00:00:00:00".parse()?, AddressType::BrEdr, 0x1007);

  println!("Connecting to {:?}", &target_sa);
  let socket = Socket::new_stream()?;
  socket.set_l2cap_opts(&create_l2cap_opts())?;

  println!("Before connection: {:?}", socket.l2cap_opts()?);
  let mut stream = socket.connect(target_sa).await.expect("connection failed");
  tokio::time::sleep(Duration::from_millis(500)).await; // If this is removed, connection fails most of the time

  println!("Local address: {:?}", stream.as_ref().local_addr()?);
  println!("Remote address: {:?}", stream.peer_addr()?);
  println!("Send MTU: {:?}", stream.as_ref().send_mtu()?);
  println!("Recv MTU: {}", stream.as_ref().recv_mtu()?);
  println!("Security: {:?}", stream.as_ref().security()?);
  println!("Flow control: {:?}", stream.as_ref().flow_control()?);
  println!("L2CAP Options: {:?}", stream.as_ref().l2cap_opts()?);

  stream.write_all(&OBEX_HELLO).await?;
  println!("Sent successfully");

  let mut buffer = vec![0; 1024];
  let n = stream.read(&mut buffer).await?;
  println!("Received: {:?}", &buffer[..n]);
  let mut conn_id: [u8; 4] = [0, 0, 0, 0];

  if buffer[0] != 0xA0 {
    eprintln!("OBEX Handshake failed.");
    panic!("OBEX Handshake failed.");
  } else {
    conn_id.copy_from_slice(&buffer[8..12]);
    println!("Connection ID: {:?}", conn_id);
  }

  if conn_id == [0, 0, 0, 0] {
    panic!("obex connection failed!!");
  }

  if conn_id == [0, 0, 0, 1] {
    panic!("this script does not support android!!");
  }

  get_ios_image(&mut stream, conn_id).await;

  Ok(())
}

async fn get_ios_image(stream: &mut Stream, conn_id: [u8; 4]) {
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
  println!("{:0x?}", req);

  let mut file = tokio::fs::File::create("/tmp/iOS.jpg").await.unwrap();

  loop {
    stream.write_all(&req).await.unwrap();
    let mut buf = [0u8; 1025];
    let n = stream.read(&mut buf).await.unwrap();
    if n == 0 {
      break;
    }
    match buf[0] {
      0x90 => file.write_all(&buf[6..n]).await.unwrap(),
      0xA0 => {
        file.write_all(&buf[6..n]).await.unwrap();
        break;
      }
      _ => panic!("obex error"),
    }
  }
}
