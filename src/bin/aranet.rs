use anyhow::{Result, anyhow, bail};
use aranet::{
    config,
    reading::{Device, Humidity, Reading},
};
use btleplug::api::{
    BDAddr, Central, CentralEvent, Manager as _, Peripheral, ScanFilter, bleuuid::uuid_from_u16,
};
use btleplug::platform::Manager;
use futures::stream::StreamExt;
use std::{collections::HashMap, str::FromStr};

static MANUFACTURER_ID: u16 = 1794;
static SERVICE_ID: u16 = 0xfce0;

async fn load_config() -> Result<config::Config> {
    let path = std::env::var("ARANET_CONFIG").unwrap_or(String::from("config.toml"));
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

    let mut last_reading: HashMap<BDAddr, Reading> = HashMap::new();

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

        let services = vec![uuid_from_u16(SERVICE_ID)];
        central.start_scan(ScanFilter { services }).await?;

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

                if let Some(last) = last_reading.get(&address) {
                    if last.is_repeat_reading(&reading) {
                        continue;
                    }
                }

                print!("aranet");
                print!(",name={}", device.name);
                print!(
                    ",device={}",
                    match reading.device {
                        Device::Aranet4 => "aranet4",
                        Device::Aranet2 => "aranet2",
                        Device::AranetRadiation => "aranet_radiation",
                        Device::AranetRadon => "aranet_radon",
                    }
                );

                print!(" ");

                if let Some(Ok(co2)) = reading.co2 {
                    print!("co2={co2}i,");
                }
                if let Some(Ok(radon)) = reading.radon {
                    print!("radon={radon}i,");
                }
                if let Ok(temperature) = reading.celsius() {
                    print!("temperature={temperature:.1},");
                }
                if let Ok(humidity) = reading.raw_humidity {
                    match humidity {
                        Humidity::V1(v) => print!("humidity={}i,", v),
                        Humidity::V2(v) => print!("humidity={:.1},", v as f32 * 0.1),
                    }
                }
                if let Ok(pressure) = reading.pressure_hpa() {
                    print!("pressure={pressure:.1},");
                }
                print!("battery={}i", reading.battery);

                print!(" ");

                let time = reading
                    .time
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap();
                let time = time.as_nanos();
                print!("{}", time);

                println!();
                last_reading.insert(address, reading);
            }
        }

        Ok(())
    });

    res.await?.await
}

#[tokio::main]
async fn main() -> Result<()> {
    let config::Config { devices } = load_config().await?;
    let devices = devices
        .into_values()
        .map(|mut device| {
            if device.name.contains('"') || device.name.contains("'") {
                bail!("Device name must not contain quotes: {}", device.name);
            }
            if device.name.contains('\\') {
                bail!("Device name must not contain backslash: {}", device.name);
            }

            // HACK: Escape spaces in device name for InfluxDB line protocol
            // Since device name isn't used much elsewhere it's okay for now.
            device.name = device.name.replace(' ', "\\ ");

            Ok(device)
        })
        .collect::<Result<Vec<_>>>()?;

    scan(devices).await?;

    Ok(())
}
