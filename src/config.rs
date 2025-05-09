use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub output: Output,
    pub devices: HashMap<String, Device>,
}

#[derive(Debug, Deserialize)]
pub struct Output {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct Device {
    pub address: String,
    pub name: String,
}

impl TryFrom<&str> for Config {
    type Error = toml::de::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        toml::from_str(value)
    }
}
