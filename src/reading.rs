#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidReading;

impl std::fmt::Display for InvalidReading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid reading")
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

#[derive(Debug, Clone)]
pub struct Reading {
    pub device: Device,
    pub co2: Option<Result<u16, InvalidReading>>,
    pub raw_temperature: Result<u16, InvalidReading>,
    pub raw_pressure: Result<u16, InvalidReading>,
    pub raw_humidity: Result<Humidity, InvalidReading>,
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

        match self.celsius() {
            Ok(v) => write!(f, "{v:.1}°C")?,
            Err(e) => write!(f, "temperature {e}")?,
        };
        write!(f, ", ")?;

        match self.raw_humidity {
            Ok(Humidity::V1(v)) => write!(f, "{v}%")?,
            Ok(Humidity::V2(v)) => write!(f, "{:.1}%", v as f32 * 0.1)?,
            Err(e) => write!(f, "{e}")?,
        };
        write!(f, ", ")?;

        match self.pressure_hpa() {
            Ok(v) => write!(f, "{v:.1}hPa")?,
            Err(e) => write!(f, "{e}")?,
        };
        write!(f, ", ")?;

        write!(f, "battery {}%", self.battery)
    }
}

impl Reading {
    pub fn celsius(&self) -> Result<f32, InvalidReading> {
        match self.raw_temperature {
            Ok(raw) => Ok(raw as f32 * 0.05),
            Err(e) => Err(e),
        }
    }

    pub fn fahrenheit(&self) -> Result<f32, InvalidReading> {
        match self.raw_temperature {
            Ok(raw) => Ok(raw as f32 * 0.05 * 9.0 / 5.0 + 32.0),
            Err(e) => Err(e),
        }
    }

    pub fn pressure_hpa(&self) -> Result<f32, InvalidReading> {
        match self.raw_pressure {
            Ok(raw) => Ok(raw as f32 * 0.1),
            Err(e) => Err(e),
        }
    }

    pub fn is_repeat_reading(&self, newer: &Reading) -> bool {
        if self.co2 != newer.co2
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
            Device::try_from(raw[0])?
        };

        match device {
            Device::AranetRadon => {
                return Err("AranetRadon is not yet supported, PRs welcome".to_string());
            }
            Device::Aranet2 => {
                return Err("Aranet2 is not yet supported, PRs welcome".to_string());
            }
            Device::AranetRadiation => {
                return Err("AranetRadiation is not yet supported, PRs welcome".to_string());
            }
            _ => {}
        };

        for _ in 0..8 {
            bytes.next();
        }

        let co2 = match device {
            Device::Aranet4 => {
                let co2 = u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);
                if (co2 >> 15) > 0 {
                    Some(Err(InvalidReading))
                } else {
                    Some(Ok(co2))
                }
            }
            _ => None,
        };

        let raw_temperature = u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);
        let raw_temperature = if ((raw_temperature >> 14) & 1) > 0 {
            Err(InvalidReading)
        } else {
            Ok(raw_temperature)
        };

        let raw_pressure = u16::from_le_bytes([*bytes.next().unwrap(), *bytes.next().unwrap()]);
        let raw_pressure = if (raw_pressure >> 15) > 0 {
            Err(InvalidReading)
        } else {
            Ok(raw_pressure)
        };

        let raw_humidity = *bytes.next().unwrap();
        let raw_humidity = if (raw_humidity >> 7) > 0 {
            Err(InvalidReading)
        } else {
            Ok(Humidity::V1(raw_humidity))
        };

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
        assert_eq!(reading.raw_temperature, Ok(452));
        assert_eq!(reading.raw_pressure, Ok(10189));
        assert_eq!(reading.raw_humidity, Ok(Humidity::V1(56)));
        assert_eq!(reading.battery, 60);
        assert_eq!(reading.interval, 60);
        assert_eq!(reading.age, 13);

        assert_eq!(reading.celsius(), Ok(22.6));
        assert_eq!(reading.fahrenheit(), Ok(72.68));
        assert_eq!(reading.pressure_hpa(), Ok(1018.9));
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
        assert!(matches!(reading.co2, Some(Err(InvalidReading))));
        assert_eq!(reading.raw_temperature, Ok(452));
        assert_eq!(reading.raw_pressure, Ok(10189));
        assert_eq!(reading.raw_humidity, Ok(Humidity::V1(56)));
        assert_eq!(reading.battery, 60);
        assert_eq!(reading.interval, 60);
        assert_eq!(reading.age, 13);
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
        assert!(matches!(reading.raw_temperature, Err(InvalidReading)));
        assert!(matches!(reading.celsius(), Err(InvalidReading)));
        assert!(matches!(reading.fahrenheit(), Err(InvalidReading)));
        assert_eq!(reading.raw_pressure, Ok(10189));
        assert_eq!(reading.raw_humidity, Ok(Humidity::V1(56)));
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
        assert_eq!(reading.raw_temperature, Ok(452));
        assert!(matches!(reading.raw_pressure, Err(InvalidReading)));
        assert!(matches!(reading.pressure_hpa(), Err(InvalidReading)));
        assert_eq!(reading.raw_humidity, Ok(Humidity::V1(56)));
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
        assert_eq!(reading.raw_temperature, Ok(452));
        assert_eq!(reading.raw_pressure, Ok(10189));
        assert!(matches!(reading.raw_humidity, Err(InvalidReading)));
        assert_eq!(reading.battery, 60);
        assert_eq!(reading.interval, 60);
        assert_eq!(reading.age, 13);
    }
}
