use anyhow::{Context, Result, anyhow, bail};
use aranet::{
    config,
    reading::{Device, Humidity, Reading},
};
use btleplug::api::{
    BDAddr, Central, CentralEvent, Manager as _, Peripheral, ScanFilter, bleuuid::uuid_from_u16,
};
use btleplug::platform::Manager;
use clap::{Parser, ValueEnum};
use futures::stream::StreamExt;
use std::{collections::HashMap, path::PathBuf, str::FromStr};

static MANUFACTURER_ID: u16 = 1794;
static SERVICE_ID: u16 = 0xfce0;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum RunMode {
    /// Print sensor readings from each configured device
    Influx,
    /// Print reachable Aranet devices
    Find,
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, env = "ARANET_CONFIG", default_value = "config.toml")]
    config_file: PathBuf,

    #[arg(long, short, default_value = "influx")]
    mode: RunMode,
}

async fn load_config(args: &Args) -> Result<config::Config> {
    let content = tokio::fs::read_to_string(&args.config_file)
        .await
        .with_context(|| format!("Failed to read config file {}", args.config_file.display()))?;
    Ok(config::Config::try_from(content.as_ref())?)
}

fn devices(config: config::Config) -> Result<HashMap<BDAddr, config::Device>> {
    config
        .devices
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

            BDAddr::from_str(&device.address)
                .map(|addr| (addr, device))
                .map_err(anyhow::Error::from)
        })
        .collect()
}

async fn scan(args: Args, config: config::Config) -> Result<()> {
    let devices = devices(config)?;
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

                match args.mode {
                    RunMode::Find => {
                        if !manufacturer_data.contains_key(&MANUFACTURER_ID) {
                            continue;
                        }

                        match (properties.local_name, devices.get(&address)) {
                            (_, Some(device)) => {
                                println!("Found configured device {} at {address}", device.name);
                            }
                            (Some(name), None) => {
                                println!("Found new device {name} at {address}");
                            }
                            (None, None) => {
                                println!("Found new unnamed device at {address}");
                            }
                        }
                        continue;
                    }
                    RunMode::Influx => {
                        // continue inline
                    }
                }

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

                if let Some(radiation) = &reading.radiation {
                    print!("radiation_rate={},", (radiation.raw_rate as f32) / 1000.0);
                    print!(
                        "radiation_total={},",
                        (radiation.raw_total as f64) / 1000000.0
                    );
                    print!("radiation_duration={}i,", radiation.raw_duration);
                }

                if let Some(Ok(temperature)) = reading.celsius() {
                    print!("temperature={temperature:.1},");
                }

                if let Some(Ok(humidity)) = reading.raw_humidity {
                    match humidity {
                        Humidity::V1(v) => print!("humidity={}i,", v),
                        Humidity::V2(v) => print!("humidity={:.1},", v as f32 * 0.1),
                    }
                }

                if let Some(Ok(pressure)) = reading.pressure_hpa() {
                    print!("pressure={pressure:.1},");
                }

                print!("battery={}i", reading.battery);

                if let Some(rssi) = properties.rssi {
                    print!(",rssi={rssi}i");
                }

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
    let args = Args::parse();
    let config = load_config(&args)
        .await
        .with_context(|| format!("Failed to load config file {}", args.config_file.display()))?;

    scan(args, config).await?;

    Ok(())
}
