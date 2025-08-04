# Power Usage

The **Power Usage** is a minimal HTTP service that fetches energy data from Prometheus and computes per-address daily power usage. It exposes a single JSON/CSV API endpoint. This service is designed for internal tools or dashboards that require historical power consumption data with minimal overhead.

## Features

* Fetches energy (`kWh`) metric from Prometheus
* Compares values between two timestamps (today vs yesterday)
* Calculates daily energy usage and average power in watts
* Supports CSV or JSON output
* Timezone-aware (WIB/GMT+7)
* Lightweight and dockerized

## Power Usage Model

```
{
  "prev_kwh": 125.4,
  "curr_kwh": 127.8,
  "daily_kwh": 2.4,
  "avg_power_watt": 100.0
}
```

> `avg_power_watt = (daily_kwh / 24) * 1000`

## API Endpoint

### `GET /api/v1/power-usage`

#### Query Parameters

| Name   | Required | Description                                     |
| ------ | -------- | ----------------------------------------------- |
| target | Yes      | Regex filter for `instance` label in Prometheus |
| date   | Yes      | Format: `YYYY-MM-DD` (local date in WIB)        |
| time   | Yes      | Format: `HH:MM` (local time in WIB)             |
| csv    | No       | If `true`, returns data as CSV                  |

#### Example (JSON):

```http
GET /api/v1/power-usage?target=192.168.1.1&date=2025-08-04&time=06:00
```

#### Example Response (JSON):

```
{
  "192.168.1.1": [
    {
      "prev_kwh": 125.4,
      "curr_kwh": 127.8,
      "daily_kwh": 2.4,
      "avg_power_watt": 100.0
    },
    ...
  ]
}
```

#### Example Response (CSV):

```
Target,Address,Prev_kWh,Current_KWh,Daily_KWh,Avg_Power_Watt
192.168.1.1,1,125.4,127.8,2.4,100.0
...
```

## Environment Variable

| Name              | Description                       | Default            |
| ----------------- | --------------------------------- | ------------------ |
| `PROMETHEUS_HOST` | The base URL of Prometheus server | (must be provided) |

Example:

```bash
export PROMETHEUS_HOST=http://localhost:9090
```

## Docker Usage

### Build Locally

```bash
docker build -t power-usage .
```

### Run the Container

```bash
docker run --init -d \
  -p 9118:9118 \
  -e PROMETHEUS_HOST=http://127.0.0.1:9090 \
  power-usage
```

## Notes

* The service runs on port **9118**
* Timezone is set to **WIB (UTC+7)**
* Requires Prometheus to expose a `energy` metric with `instance` and `address` labels
* Uses the latest 10-minute data point via `last_over_time(...)`

## Error Handling

| Status Code        | Reason                              |
| ------------------ | ----------------------------------- |
| 400 Bad Request    | Missing or invalid query parameters |
| 502 Bad Gateway    | Prometheus unreachable or invalid   |
| 500 Internal Error | Internal computation failure        |

