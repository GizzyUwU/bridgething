#[cfg(debug_assertions)]
pub async fn query_adapter(adapter: &bluer::Adapter) -> bluer::Result<()> {
  println!("Debug Adapter Information:");
  println!("Address:                    {}", adapter.address().await?);
  println!("Address type:               {}", adapter.address_type().await?);
  println!("Friendly name:              {}", adapter.alias().await?);
  println!("Modalias:                   {:?}", adapter.modalias().await?);
  println!("Powered:                    {:?}", adapter.is_powered().await?);
  println!("Discoverabe:                {:?}", adapter.is_discoverable().await?);
  println!("Pairable:                   {:?}", adapter.is_pairable().await?);
  println!("UUIDs:                      {:?}\n", adapter.uuids().await?);
  println!(
    "Active adv. instances:      {}",
    adapter.active_advertising_instances().await?
  );
  println!(
    "Supp.  adv. instances:      {}",
    adapter.supported_advertising_instances().await?
  );
  println!(
    "Supp.  adv. includes:       {:?}",
    adapter.supported_advertising_system_includes().await?
  );
  println!(
    "Adv. capabilites:           {:?}",
    adapter.supported_advertising_capabilities().await?
  );
  println!(
    "Adv. features:              {:?}\n",
    adapter.supported_advertising_features().await?
  );

  println!("Adapter Properties:");
  let props = adapter.all_properties().await?;
  for prop in props {
    println!("Property:                   {:?}", &prop);
  }

  Ok(())
}

#[cfg(debug_assertions)]
pub async fn query_device(device: &bluer::Device) -> bluer::Result<()> {
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

  let props = device.all_properties().await?;
  for prop in props {
    println!("    {:?}", &prop);
  }
  Ok(())
}
