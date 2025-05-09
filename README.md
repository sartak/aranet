# Aranet4 exporter for InfluxDB / victoria-metrics

This is a service that listens for Bluetooth advertisement data from Aranet4
and publishes them to a time-series database like InfluxDB or victoria-metrics.

You do not need to pair the Aranet4 device with the host running this service.
However, you do need to enable "Smart Home integrations" for each Aranet4 device,
and you'll need its Bluetooth MAC address.

It expects a configuration file named by the `$ARANET_CONFIG` environment
variable (or, if unset, `config.toml` in the current directory). The config
file has the following format:

```toml
[output]
url = "http://victoria-metrics:8428"

[devices]
11111 = { address = "01:23:45:67:89:AB", name = "Living room" }
22222 = { address = "CD:EF:01:23:45:67", name = "Kitchen" }
33333 = { address = "89:AB:CD:EF:01:23", name = "Bedroom" }
```

See also [Aranet4-Python](https://github.com/Anrijs/Aranet4-Python) which is
more feature complete.
