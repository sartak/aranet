# Aranet4 exporter for InfluxDB / victoria-metrics

This is a service that listens for Bluetooth advertisement data from Aranet4
and prints each one, intended for a time-series database like InfluxDB or
victoria-metrics.

You do not need to pair your Aranet4 device with the host running this service.
However, you do need to enable "Smart Home integrations" for each Aranet4 device,
and you'll need its Bluetooth MAC address.

This service expects a configuration file named by the `$ARANET_CONFIG`
environment variable (or, if unset, `config.toml` in the current directory).
The config file has the following format:

```toml
[devices]
11111 = { address = "01:23:45:67:89:AB", name = "Living room" }
22222 = { address = "CD:EF:01:23:45:67", name = "Kitchen" }
33333 = { address = "89:AB:CD:EF:01:23", name = "Bedroom" }
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
