use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingError {
    Invalid,
    NoData,
    HighHumidity,
}

impl std::fmt::Display for ReadingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ReadingError::*;
        match self {
            Invalid => write!(f, "Invalid reading"),
            NoData => write!(f, "No data"),
            HighHumidity => write!(f, "Humidity too high"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Humidity {
    V1(u8),
    V2(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Aranet4,
    Aranet2,
    AranetRadiation,
    AranetRadon,
}

impl std::fmt::Display for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Device::*;
        match self {
            Aranet4 => write!(f, "Aranet4"),
            Aranet2 => write!(f, "Aranet2"),
            AranetRadiation => write!(f, "AranetRadiation"),
            AranetRadon => write!(f, "AranetRadon"),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Radiation {
    pub raw_total: u32,
    pub raw_duration: u32,
    pub raw_rate: u16,
}

impl Radiation {
    pub fn duration(&self) -> Duration {
        Duration::from_secs(self.raw_duration as u64)
    }

    pub fn duration_string(&self) -> String {
        let duration = self.raw_duration;
        let hours = duration / 3600;
        let minutes = (duration % 3600) / 60;
        let seconds = duration % 60;

        let str = [(hours, "h"), (minutes, "m"), (seconds, "s")]
            .iter()
            .filter(|(v, _)| *v > 0)
            .map(|(v, s)| format!("{v}{s}"))
            .collect::<Vec<_>>()
            .join(" ");

        if str.is_empty() {
            String::from("0s")
        } else {
            str
        }
    }
}

#[derive(Debug, Clone)]
pub struct Reading {
    pub device: Device,
    pub co2: Option<Result<u16, ReadingError>>,
    pub radon: Option<Result<u16, ReadingError>>,
    pub radiation: Option<Radiation>,
    pub raw_temperature: Option<Result<u16, ReadingError>>,
    pub raw_pressure: Option<Result<u16, ReadingError>>,
    pub raw_humidity: Option<Result<Humidity, ReadingError>>,
    pub battery: u8,
    pub interval: u16,
    pub age: u16,
    pub instant: std::time::Instant,
    pub time: std::time::SystemTime,
}

impl TryFrom<u8> for Device {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Device::Aranet4),
            1 => Ok(Device::Aranet2),
            2 => Ok(Device::AranetRadiation),
            3 => Ok(Device::AranetRadon),
            _ => Err(format!("Unknown device type: {value}")),
        }
    }
}

impl std::fmt::Display for Reading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(co2) = self.co2 {
            write!(f, "CO₂ ")?;
            match co2 {
                Ok(v) => write!(f, "{v}ppm")?,
                Err(e) => write!(f, "{e}")?,
            };
            write!(f, ", ")?;
        }

        if let Some(radon) = self.radon {
            write!(f, "radon ")?;
            match radon {
                Ok(v) => write!(f, "{v}Bq/m³")?,
                Err(e) => write!(f, "{e}")?,
            };
            write!(f, ", ")?;
        }

        if let Some(radiation) = &self.radiation {
            write!(
                f,
                "radiation {:.3} µSv/h ({:.6} mSv in {})",
                (radiation.raw_rate as f32) / 1000.0,
                (radiation.raw_total as f64) / 1000000.0,
                radiation.duration_string(),
            )?;
            write!(f, ", ")?;
        }

        if let Some(celsius) = self.celsius() {
            match celsius {
                Ok(v) => write!(f, "{v:.1}°C")?,
                Err(e) => write!(f, "temperature {e}")?,
            };
            write!(f, ", ")?;
        }

        if let Some(humidity) = self.raw_humidity {
            match humidity {
                Ok(Humidity::V1(v)) => write!(f, "{v}%")?,
                Ok(Humidity::V2(v)) => write!(f, "{:.1}%", v as f32 * 0.1)?,
                Err(e) => write!(f, "{e}")?,
            };
            write!(f, ", ")?;
        }

        if let Some(pressure) = self.pressure_hpa() {
            match pressure {
                Ok(v) => write!(f, "{v:.1}hPa")?,
                Err(e) => write!(f, "{e}")?,
            };
            write!(f, ", ")?;
        }

        write!(f, "battery {}%", self.battery)
    }
}

impl Reading {
    pub fn celsius(&self) -> Option<Result<f32, ReadingError>> {
        self.raw_temperature.map(|t| match t {
            Ok(raw) => Ok(raw as f32 * 0.05),
            Err(e) => Err(e),
        })
    }

    pub fn fahrenheit(&self) -> Option<Result<f32, ReadingError>> {
        self.raw_temperature.map(|t| match t {
            Ok(raw) => Ok(raw as f32 * 0.05 * 9.0 / 5.0 + 32.0),
            Err(e) => Err(e),
        })
    }

    pub fn pressure_hpa(&self) -> Option<Result<f32, ReadingError>> {
        self.raw_pressure.map(|p| match p {
            Ok(raw) => Ok(raw as f32 * 0.1),
            Err(e) => Err(e),
        })
    }

    pub fn is_repeat_reading(&self, newer: &Reading) -> bool {
        if self.co2 != newer.co2
            || self.radon != newer.radon
            || self.radiation != newer.radiation
            || self.raw_temperature != newer.raw_temperature
            || self.raw_pressure != newer.raw_pressure
            || self.raw_humidity != newer.raw_humidity
            || self.battery != newer.battery
        {
            // New sensor data, definitely a new reading
            return false;
        }

        if newer.age < self.age {
            // The clock rolled over, definitely a new reading
            return false;
        }

        if newer.interval != self.interval {
            // The interval changed. That doesn't tell us anything about whether
            // there's a new reading, but it does simplify the next check.
            // Since this is rare, we'll just assume it's a repeat reading. (The
            // sensor data is the same after all)
            return true;
        }

        let secs = newer.instant.duration_since(self.instant).as_secs();
        if secs > newer.interval as u64 {
            // If it's been longer than the interval, then we can assume a new
            // reading with the same values.
            return false;
        }

        true
    }
}

impl TryFrom<&[u8]> for Reading {
    type Error = String;

    fn try_from(raw: &[u8]) -> Result<Self, Self::Error> {
        if raw.len() < 21 {
            return Err(format!(
                "Raw reading data too short, expected 21 bytes, got {}",
                raw.len(),
            ));
        }

        let mut bytes = raw.iter();

        // Aranet4 doesn't identify itself the same way
        let device = if raw.len() == 22 {
            Device::Aranet4
        } else {
            Device::try_from(*bytes.next().unwrap())?
        };

        if device == Device::Aranet2 {
            return Err("Aranet2 is not yet supported, PRs welcome".to_string());
        };

        let skip = match device {
            Device::Aranet4 => 8,
            Device::AranetRadon => 7,
            Device::AranetRadiation => 5,
            Device::Aranet2 => unreachable!(),
        };

        for _ in 0..skip {
            bytes.next();
        }

        let co2 = match device {
            Device::Aranet4 => {
                let co2 = u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);
                if (co2 >> 15) > 0 {
                    Some(Err(ReadingError::Invalid))
                } else {
                    Some(Ok(co2))
                }
            }
            _ => None,
        };

        let radon = match device {
            Device::AranetRadon => {
                let radon = u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);
                if radon == 0x1F01 {
                    Some(Err(ReadingError::NoData))
                } else if radon == 0x1F02 {
                    Some(Err(ReadingError::HighHumidity))
                } else if radon > 0x1f00 {
                    Some(Err(ReadingError::Invalid))
                } else {
                    Some(Ok(radon))
                }
            }
            _ => None,
        };

        let radiation = match device {
            Device::AranetRadiation => {
                let raw_total = u32::from_le_bytes([
                    *bytes.next().unwrap(),
                    *bytes.next().unwrap(),
                    *bytes.next().unwrap(),
                    *bytes.next().unwrap(),
                ]);
                let raw_duration = u32::from_le_bytes([
                    *bytes.next().unwrap(),
                    *bytes.next().unwrap(),
                    *bytes.next().unwrap(),
                    *bytes.next().unwrap(),
                ]);
                let raw_rate = u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);

                bytes.next();

                Some(Radiation {
                    raw_total,
                    raw_duration,
                    raw_rate,
                })
            }
            _ => None,
        };

        let raw_temperature = match device {
            Device::Aranet4 | Device::AranetRadon => {
                let raw_temperature =
                    u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);
                let raw_temperature = if ((raw_temperature >> 14) & 1) > 0 {
                    Err(ReadingError::Invalid)
                } else {
                    Ok(raw_temperature)
                };
                Some(raw_temperature)
            }
            Device::AranetRadiation => None,
            Device::Aranet2 => unreachable!(),
        };

        let raw_pressure = match device {
            Device::Aranet4 | Device::AranetRadon => {
                let raw_pressure =
                    u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);
                let raw_pressure = if (raw_pressure >> 15) > 0 {
                    Err(ReadingError::Invalid)
                } else {
                    Ok(raw_pressure)
                };
                Some(raw_pressure)
            }
            Device::AranetRadiation => None,
            Device::Aranet2 => unreachable!(),
        };

        let raw_humidity = match device {
            Device::Aranet4 => {
                let raw_humidity = *bytes.next().unwrap();
                if (raw_humidity >> 7) > 0 {
                    Some(Err(ReadingError::Invalid))
                } else {
                    Some(Ok(Humidity::V1(raw_humidity)))
                }
            }
            Device::AranetRadon => {
                let raw_humidity =
                    u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);
                if (raw_humidity >> 15) > 0 {
                    Some(Err(ReadingError::Invalid))
                } else {
                    Some(Ok(Humidity::V2(raw_humidity)))
                }
            }
            Device::AranetRadiation => None,
            Device::Aranet2 => unreachable!(),
        };

        match device {
            Device::Aranet4 => {}
            Device::AranetRadon => {
                bytes.next();
            }
            Device::AranetRadiation => {}
            Device::Aranet2 => unreachable!(),
        }

        let battery = *bytes.next().unwrap();
        let _status = *bytes.next().unwrap();

        let interval = u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);
        let age = u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);

        let instant = std::time::Instant::now();
        let instant = instant
            .checked_sub(std::time::Duration::from_secs(age as u64))
            .ok_or_else(|| "Failed to get current instant".to_string())?;

        let time = std::time::SystemTime::now();
        let time = time
            .checked_sub(std::time::Duration::from_secs(age as u64))
            .ok_or_else(|| "Failed to get current time".to_string())?;

        Ok(Reading {
            device,
            co2,
            radon,
            radiation,
            raw_temperature,
            raw_pressure,
            raw_humidity,
            battery,
            interval,
            age,
            instant,
            time,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_co2_reading() {
        let raw = vec![
            0x21, 0x2c, 0x05, 0x01, 0x00, 0x0c, 0x01, 0x01, 0xf0, 0x02, 0xc4, 0x01, 0xcd, 0x27,
            0x38, 0x3c, 0x01, 0x3c, 0x00, 0x0d, 0x00, 0x5d,
        ];

        let reading = Reading::try_from(raw.as_slice()).unwrap();
        assert_eq!(reading.device, Device::Aranet4);
        assert_eq!(reading.co2, Some(Ok(752)));
        assert_eq!(reading.radon, None);
        assert_eq!(reading.radiation, None);
        assert_eq!(reading.raw_temperature, Some(Ok(452)));
        assert_eq!(reading.raw_pressure, Some(Ok(10189)));
        assert_eq!(reading.raw_humidity, Some(Ok(Humidity::V1(56))));
        assert_eq!(reading.battery, 60);
        assert_eq!(reading.interval, 60);
        assert_eq!(reading.age, 13);

        assert_eq!(reading.celsius(), Some(Ok(22.6)));
        assert_eq!(reading.fahrenheit(), Some(Ok(72.68)));
        assert_eq!(reading.pressure_hpa(), Some(Ok(1018.9)));
    }

    #[test]
    fn test_radon_reading() {
        let raw = vec![
            0x03, 0x21, 0x04, 0x09, 0x01, 0x00, 0x00, 0x00, 0x18, 0x00, 0x4c, 0x01, 0x50, 0x27,
            0x35, 0x02, 0x00, 0x64, 0x01, 0x58, 0x02, 0x41, 0x01, 0x45,
        ];

        let reading = Reading::try_from(raw.as_slice()).unwrap();
        assert_eq!(reading.device, Device::AranetRadon);
        assert_eq!(reading.co2, None);
        assert_eq!(reading.radon, Some(Ok(24)));
        assert_eq!(reading.radiation, None);
        assert_eq!(reading.raw_temperature, Some(Ok(332)));
        assert_eq!(reading.raw_pressure, Some(Ok(10064)));
        assert_eq!(reading.raw_humidity, Some(Ok(Humidity::V2(565))));
        assert_eq!(reading.battery, 100);
        assert_eq!(reading.interval, 600);
        assert_eq!(reading.age, 321);

        assert_eq!(reading.celsius(), Some(Ok(16.6)));
        assert_eq!(reading.fahrenheit(), Some(Ok(61.88)));
        assert_eq!(reading.pressure_hpa(), Some(Ok(1006.4)));
    }

    #[test]
    fn test_radiation_reading() {
        let raw = vec![
            0x02, 0x21, 0x01, 0x09, 0x01, 0x00, 0x35, 0x00, 0x00, 0x00, 0xe4, 0x0c, 0x00, 0x00,
            0x3c, 0x00, 0x00, 0x64, 0x00, 0x3c, 0x00, 0x05, 0x00, 0x37,
        ];

        let reading = Reading::try_from(raw.as_slice()).unwrap();
        assert_eq!(reading.device, Device::AranetRadiation);
        assert_eq!(reading.co2, None);
        assert_eq!(reading.radon, None);
        assert_eq!(
            reading.radiation,
            Some(Radiation {
                raw_total: 53,
                raw_duration: 3300,
                raw_rate: 60,
            })
        );
        assert_eq!(reading.raw_temperature, None);
        assert_eq!(reading.raw_pressure, None);
        assert_eq!(reading.raw_humidity, None);
        assert_eq!(reading.battery, 100);
        assert_eq!(reading.interval, 60);
        assert_eq!(reading.age, 5);

        assert_eq!(reading.celsius(), None);
        assert_eq!(reading.fahrenheit(), None);
        assert_eq!(reading.pressure_hpa(), None);
    }

    #[test]
    fn test_short() {
        let raw = vec![
            0x21, 0x2c, 0x05, 0x01, 0x00, 0x0c, 0x01, 0x01, 0xf0, 0x02, 0xc4, 0x01, 0xcd, 0x27,
            0x38, 0x3c, 0x01, 0x3c, 0x00, 0x0d,
        ];

        assert!(Reading::try_from(raw.as_slice()).is_err());
    }

    #[test]
    fn test_invalid_co2() {
        let raw = vec![
            0x21, 0x2c, 0x05, 0x01, 0x00, 0x0c, 0x01, 0x01, 0xff, 0xff, 0xc4, 0x01, 0xcd, 0x27,
            0x38, 0x3c, 0x01, 0x3c, 0x00, 0x0d, 0x00, 0x5d,
        ];

        let reading = Reading::try_from(raw.as_slice()).unwrap();
        assert_eq!(reading.device, Device::Aranet4);
        assert_eq!(reading.co2, Some(Err(ReadingError::Invalid)));
        assert_eq!(reading.radon, None);
        assert_eq!(reading.radiation, None);
        assert_eq!(reading.raw_temperature, Some(Ok(452)));
        assert_eq!(reading.raw_pressure, Some(Ok(10189)));
        assert_eq!(reading.raw_humidity, Some(Ok(Humidity::V1(56))));
        assert_eq!(reading.battery, 60);
        assert_eq!(reading.interval, 60);
        assert_eq!(reading.age, 13);
    }

    #[test]
    fn test_invalid_radon() {
        let raw = vec![
            0x03, 0x21, 0x04, 0x09, 0x01, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x4c, 0x01, 0x50, 0x27,
            0x35, 0x02, 0x00, 0x64, 0x01, 0x58, 0x02, 0x41, 0x01, 0x45,
        ];

        let reading = Reading::try_from(raw.as_slice()).unwrap();
        assert_eq!(reading.device, Device::AranetRadon);
        assert_eq!(reading.co2, None);
        assert_eq!(reading.radon, Some(Err(ReadingError::Invalid)));
        assert_eq!(reading.radiation, None);
        assert_eq!(reading.raw_temperature, Some(Ok(332)));
        assert_eq!(reading.raw_pressure, Some(Ok(10064)));
        assert_eq!(reading.raw_humidity, Some(Ok(Humidity::V2(565))));
        assert_eq!(reading.battery, 100);
        assert_eq!(reading.interval, 600);
        assert_eq!(reading.age, 321);

        assert_eq!(reading.celsius(), Some(Ok(16.6)));
        assert_eq!(reading.fahrenheit(), Some(Ok(61.88)));
        assert_eq!(reading.pressure_hpa(), Some(Ok(1006.4)));
    }

    #[test]
    fn test_invalid_radon_nodata() {
        let raw = vec![
            0x03, 0x21, 0x04, 0x09, 0x01, 0x00, 0x00, 0x00, 0x01, 0x1F, 0x4c, 0x01, 0x50, 0x27,
            0x35, 0x02, 0x00, 0x64, 0x01, 0x58, 0x02, 0x41, 0x01, 0x45,
        ];

        let reading = Reading::try_from(raw.as_slice()).unwrap();
        assert_eq!(reading.device, Device::AranetRadon);
        assert_eq!(reading.co2, None);
        assert_eq!(reading.radon, Some(Err(ReadingError::NoData)));
        assert_eq!(reading.radiation, None);
        assert_eq!(reading.raw_temperature, Some(Ok(332)));
        assert_eq!(reading.raw_pressure, Some(Ok(10064)));
        assert_eq!(reading.raw_humidity, Some(Ok(Humidity::V2(565))));
        assert_eq!(reading.battery, 100);
        assert_eq!(reading.interval, 600);
        assert_eq!(reading.age, 321);

        assert_eq!(reading.celsius(), Some(Ok(16.6)));
        assert_eq!(reading.fahrenheit(), Some(Ok(61.88)));
        assert_eq!(reading.pressure_hpa(), Some(Ok(1006.4)));
    }

    #[test]
    fn test_invalid_radon_highhumidity() {
        let raw = vec![
            0x03, 0x21, 0x04, 0x09, 0x01, 0x00, 0x00, 0x00, 0x02, 0x1F, 0x4c, 0x01, 0x50, 0x27,
            0x35, 0x02, 0x00, 0x64, 0x01, 0x58, 0x02, 0x41, 0x01, 0x45,
        ];

        let reading = Reading::try_from(raw.as_slice()).unwrap();
        assert_eq!(reading.device, Device::AranetRadon);
        assert_eq!(reading.co2, None);
        assert_eq!(reading.radon, Some(Err(ReadingError::HighHumidity)));
        assert_eq!(reading.radiation, None);
        assert_eq!(reading.raw_temperature, Some(Ok(332)));
        assert_eq!(reading.raw_pressure, Some(Ok(10064)));
        assert_eq!(reading.raw_humidity, Some(Ok(Humidity::V2(565))));
        assert_eq!(reading.battery, 100);
        assert_eq!(reading.interval, 600);
        assert_eq!(reading.age, 321);

        assert_eq!(reading.celsius(), Some(Ok(16.6)));
        assert_eq!(reading.fahrenheit(), Some(Ok(61.88)));
        assert_eq!(reading.pressure_hpa(), Some(Ok(1006.4)));
    }

    #[test]
    fn test_invalid_raw_temperature() {
        let raw = vec![
            0x21, 0x2c, 0x05, 0x01, 0x00, 0x0c, 0x01, 0x01, 0xf0, 0x02, 0xff, 0xff, 0xcd, 0x27,
            0x38, 0x3c, 0x01, 0x3c, 0x00, 0x0d, 0x00, 0x5d,
        ];

        let reading = Reading::try_from(raw.as_slice()).unwrap();
        assert_eq!(reading.device, Device::Aranet4);
        assert_eq!(reading.co2, Some(Ok(752)));
        assert_eq!(reading.radon, None);
        assert_eq!(reading.radiation, None);
        assert_eq!(reading.raw_temperature, Some(Err(ReadingError::Invalid)));
        assert_eq!(reading.celsius(), Some(Err(ReadingError::Invalid)));
        assert_eq!(reading.fahrenheit(), Some(Err(ReadingError::Invalid)));
        assert_eq!(reading.raw_pressure, Some(Ok(10189)));
        assert_eq!(reading.raw_humidity, Some(Ok(Humidity::V1(56))));
        assert_eq!(reading.battery, 60);
        assert_eq!(reading.interval, 60);
        assert_eq!(reading.age, 13);
    }

    #[test]
    fn test_invalid_pressure() {
        let raw = vec![
            0x21, 0x2c, 0x05, 0x01, 0x00, 0x0c, 0x01, 0x01, 0xf0, 0x02, 0xc4, 0x01, 0xff, 0xff,
            0x38, 0x3c, 0x01, 0x3c, 0x00, 0x0d, 0x00, 0x5d,
        ];

        let reading = Reading::try_from(raw.as_slice()).unwrap();
        assert_eq!(reading.device, Device::Aranet4);
        assert_eq!(reading.co2, Some(Ok(752)));
        assert_eq!(reading.radon, None);
        assert_eq!(reading.radiation, None);
        assert_eq!(reading.raw_temperature, Some(Ok(452)));
        assert_eq!(reading.raw_pressure, Some(Err(ReadingError::Invalid)));
        assert_eq!(reading.pressure_hpa(), Some(Err(ReadingError::Invalid)));
        assert_eq!(reading.raw_humidity, Some(Ok(Humidity::V1(56))));
        assert_eq!(reading.battery, 60);
        assert_eq!(reading.interval, 60);
        assert_eq!(reading.age, 13);
    }

    #[test]
    fn test_invalid_humidity() {
        let raw = vec![
            0x21, 0x2c, 0x05, 0x01, 0x00, 0x0c, 0x01, 0x01, 0xf0, 0x02, 0xc4, 0x01, 0xcd, 0x27,
            0xff, 0x3c, 0x01, 0x3c, 0x00, 0x0d, 0x00, 0x5d,
        ];

        let reading = Reading::try_from(raw.as_slice()).unwrap();
        assert_eq!(reading.device, Device::Aranet4);
        assert_eq!(reading.co2, Some(Ok(752)));
        assert_eq!(reading.radon, None);
        assert_eq!(reading.radiation, None);
        assert_eq!(reading.raw_temperature, Some(Ok(452)));
        assert_eq!(reading.raw_pressure, Some(Ok(10189)));
        assert_eq!(reading.raw_humidity, Some(Err(ReadingError::Invalid)));
        assert_eq!(reading.battery, 60);
        assert_eq!(reading.interval, 60);
        assert_eq!(reading.age, 13);
    }
}
