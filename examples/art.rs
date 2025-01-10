use bluer::{
  l2cap::{SocketAddr, Stream},
  Address, AddressType,
};
use tokio::io::AsyncReadExt;
use tokio_util::bytes::BytesMut;

#[tokio::main]
async fn main() -> bluer::Result<()> {
  let session = bluer::Session::new().await?;
  let adapter = session.default_adapter().await?;
  adapter.set_powered(true).await?;

  let target_addr: Address = "70:B3:06:14:A9:78".parse().expect("failed to parse address");
  let target_sa = SocketAddr::new(target_addr, AddressType::BrEdr, 4103);

  println!("Connecting to {:?}", &target_sa);
  let mut stream = Stream::connect(target_sa).await.expect("connection failed");
  println!("Local address: {:?}", stream.as_ref().local_addr()?);
  println!("Remote address: {:?}", stream.peer_addr()?);
  println!("Send MTU: {:?}", stream.as_ref().send_mtu());
  println!("Recv MTU: {}", stream.as_ref().recv_mtu()?);
  println!("Security: {:?}", stream.as_ref().security()?);
  println!("Flow control: {:?}", stream.as_ref().flow_control());

  println!("\nReceiving");

  loop {
    let mut buf = BytesMut::new();
    let bytes = stream.read_buf(&mut buf).await.expect("read failed");

    if bytes > 0 {
      println!("Received: {}", String::from_utf8_lossy(&buf));
    }
  }

  // println!("Done");

  // Ok(())
}
