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

  // let mut events = device.events().await.unwrap();
  // if let Some(ev) = events.next().await {
  //     println!("On device {:?}, received event {:?}", device, ev);
  // }

  println!("device services resolved");

  for service in device.services().await? {
    let uuid = service.uuid().await?;
    println!("service UUID: {}", &uuid);
    println!("service {:?}\n", &service.all_properties().await?);

    for char in service.characteristics().await? {
      let uuid = char.uuid().await?;
      println!("characteristic uuid: {}", &uuid);
      println!("characteristic descriptors: {:?}", &char.descriptors().await?);
      println!("characteristic: {:?}", &char.all_properties().await?);
    }

    println!("\n");
  }

  Ok(())
}
