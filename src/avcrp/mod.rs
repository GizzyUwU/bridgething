async fn testing() -> bluer::Result<()> {
  let session = bluer::Session::new().await?;
  let adapter_names = session.adapter_names().await?;

  let adapter = session.adapter(&adapter_names[0])?;
  println!("Bluetooth adapater {}:", &adapter_names[0]);
  let props: Vec<bluer::AdapterProperty> = adapter.all_properties().await?;
  for prop in props {
    println!("    {:?}", &prop);
  }

  let address = "".parse().expect("failed to parse bluetooth address");
  let device = adapter.device(address)?;
  println!("    Address type:       {}", device.address_type().await?);
  println!("    Name:               {:?}", device.name().await?);
  println!("    Icon:               {:?}", device.icon().await?);
  println!("    Class:              {:?}", device.class().await?);
  println!(
    "    UUIDs:              {:?}",
    device.uuids().await?.unwrap_or_default()
  );
  println!("    Paired:             {:?}", device.is_paired().await?);
  println!("    Connected:          {:?}", device.is_connected().await?);
  println!("    Trusted:            {:?}", device.is_trusted().await?);
  println!("    Modalias:           {:?}", device.modalias().await?);
  println!("    RSSI:               {:?}", device.rssi().await?);
  println!("    TX power:           {:?}", device.tx_power().await?);
  println!("    Manufacturer data:  {:?}", device.manufacturer_data().await?);
  println!("    Service data:       {:?}", device.service_data().await?);

  if !device.is_connected().await? {
    device.connect().await?;
  }

  let socket_addr = bluer::l2cap::SocketAddr::new(address, bluer::AddressType::BrEdr, 0x0017);
  println!("Connecting to {:?}", &socket_addr);
  let mut stream = bluer::l2cap::Stream::connect(socket_addr)
    .await
    .expect("connection failed");
  println!("Local address: {:?}", stream.as_ref().local_addr()?);
  println!("Remote address: {:?}", stream.peer_addr()?);
  println!("Send MTU: {:?}", stream.as_ref().send_mtu());
  println!("Recv MTU: {}", stream.as_ref().recv_mtu()?);
  println!("Security: {:?}", stream.as_ref().security()?);
  println!("Flow control: {:?}", stream.as_ref().flow_control());

  let (mut rh, mut wh) = stream.into_split();

  loop {}

  Ok(())
}
