# Aranet4 exporter for InfluxDB / victoria-metrics

![Example Grafana dashboard of Aranet sensor output](grafana.png)
*Example Grafana dashboard of Aranet sensor output*

This is a service that listens for Bluetooth advertisement data from Aranet4,
AranetRadon, and AranetRadiation. It prints each measurement, intended for a
time-series database like InfluxDB or victoria-metrics. PRs welcome for other
sensors.

You do not need to pair your Aranet4 device with the host running this service.
However, you do need to enable "Smart Home integrations" for each Aranet4
device, and you'll need its Bluetooth MAC address. To help find these MAC
addresses, you can pass `--mode=find` to print each time a payload is received
from any Aranet device.

This service expects a configuration file, which is named by the following
order of precedence:

1. the `--config-file` flag
2. the `ARANET_CONFIG` environment variable
3. the default of `config.toml` in the current directory

The config file has the following format. The `11111`-style keys are unused by
this service, but are intended to be the Aranet device IDs.

```toml
[devices]
11111 = { address = "01:23:45:67:89:AB", name = "Living room" }
22222 = { address = "CD:EF:01:23:45:67", name = "Kitchen" }
33333 = { address = "89:AB:CD:EF:01:23", name = "Bedroom" }
```

Here's an example of the output (which uses the
[InfluxDB line protocol](https://docs.influxdata.com/influxdb/v2/reference/syntax/line-protocol/)). Three of the rooms use a CO₂ sensor while "Basement" uses
radon and radiation too:

```
aranet,name=Kitchen,device=aranet4 co2=485i,temperature=15.9,humidity=53i,pressure=1008.7,battery=60i,rssi=-65i 1746888802038113983
aranet,name=Foyer,device=aranet4 co2=534i,temperature=18.4,humidity=58i,pressure=1006.3,battery=57i,rssi=-70i 1746888812063136890
aranet,name=Basement,device=aranet_radon radon=32i,temperature=16.6,humidity=58.2,pressure=1006.6,battery=100i,rssi=-78i 1746888800079903620
aranet,name=Dining\ room,device=aranet4 co2=557i,temperature=15.9,humidity=58i,pressure=1006.7,battery=57i,rssi=-73i 1746888838490242786
aranet,name=Dining\ room,device=aranet4 co2=549i,temperature=15.9,humidity=58i,pressure=1006.7,battery=58i,rssi=-73i 1746888860106787942
aranet,name=Basement,device=aranet_radiation radiation_rate=0.03,radiation_total=0.000048,radiation_duration=3420i,battery=100i,rssi=-77i 1746888866024767385
aranet,name=Kitchen,device=aranet4 co2=486i,temperature=16.0,humidity=53i,pressure=1008.8,battery=60i,rssi=-63i 1746888871942746828
```

You'll want to use a tool like `telegraf` to publish the data from this service
into your time-series database.

```toml
[[inputs.execd]]
  command = ["/path/to/aranet"]
  environment = ["ARANET_CONFIG=/path/to/config.toml"]
  signal = "none"
  restart_delay = "10s"
  data_format = "influx"
```

See also [Aranet4-Python](https://github.com/Anrijs/Aranet4-Python) which is
more feature complete.
