use anyhow::{Result, anyhow};
use aranet::{config, reading::Reading};
use btleplug::api::{BDAddr, Central, CentralEvent, Manager as _, Peripheral};
use btleplug::platform::Manager;
use futures::stream::StreamExt;
use std::{collections::HashMap, str::FromStr};

static MANUFACTURER_ID: u16 = 1794;

async fn load_config() -> Result<config::Config> {
    let path = "config.toml";
    let content = tokio::fs::read_to_string(path).await?;
    Ok(config::Config::try_from(content.as_ref())?)
}

async fn scan(devices: Vec<config::Device>) -> Result<()> {
    let devices = devices
        .into_iter()
        .map(|device| {
            BDAddr::from_str(&device.address)
                .map(|addr| (addr, device))
                .map_err(anyhow::Error::from)
        })
        .collect::<Result<HashMap<BDAddr, config::Device>>>()?;

    let res = tokio::task::spawn_blocking(async move || -> Result<()> {
        let manager = Manager::new().await?;

        let adapters = manager.adapters().await?;
        let central = adapters
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No Bluetooth adapters found"))?;

        let central_state = central.adapter_state().await?;
        if central_state != btleplug::api::CentralState::PoweredOn {
            return Err(anyhow!("Bluetooth adapter is not powered on"));
        }

        let mut events = central.events().await?;

        while let Some(event) = events.next().await {
            if let CentralEvent::ManufacturerDataAdvertisement {
                id,
                manufacturer_data,
            } = event
            {
                let peripheral = match central.peripheral(&id).await {
                    Ok(peripheral) => peripheral,
                    Err(e) => {
                        eprintln!("Error getting peripheral for {id}: {e:?}");
                        continue;
                    }
                };

                let properties = match peripheral.properties().await {
                    Ok(Some(properties)) => properties,
                    Ok(None) => {
                        eprintln!("No properties for {id}");
                        continue;
                    }
                    Err(e) => {
                        eprintln!("Error getting properties for {id}: {e:?}");
                        continue;
                    }
                };

                let address = properties.address;
                let Some(device) = devices.get(&address) else {
                    continue;
                };

                let payload = match manufacturer_data.get(&MANUFACTURER_ID) {
                    Some(payload) => payload,
                    None => {
                        eprintln!(
                            "No manufacturer data from {}: {:?}",
                            device.name, manufacturer_data
                        );
                        continue;
                    }
                };

                let reading = match Reading::try_from(payload.as_slice()) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!(
                            "Failed to parse payload from {}: {:?} {:?}",
                            device.name, e, payload
                        );
                        continue;
                    }
                };

                println!("{}: {}", device.name, reading);
            }
        }

        Ok(())
    });

    res.await?.await
}

#[tokio::main]
async fn main() -> Result<()> {
    let config::Config { output: _, devices } = load_config().await?;
    let devices = devices.into_values().collect::<Vec<_>>();
    scan(devices).await?;

    Ok(())
}
