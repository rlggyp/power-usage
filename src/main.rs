use axum::{
    extract::Query,
    response::IntoResponse,
    routing::get,
    Router,
    http::StatusCode,
    Json,
};
use chrono::{NaiveDate, FixedOffset, DateTime, Utc, Duration};
use serde::Serialize;
use std::{collections::HashMap, net::SocketAddr, sync::OnceLock};
use serde_json::Value;

const HOUR: i32 = 3600;
static PROMETHEUS_HOST: OnceLock<String> = OnceLock::new();

#[derive(Serialize)]
struct PowerUsage {
    prev_kwh: f64,
    curr_kwh: f64,
    daily_kwh: f64,
    avg_power_watt: f64,
}

#[tokio::main]
async fn main() {
    let prometheus_host = std::env::var("PROMETHEUS_HOST").expect("`PROMETHEUS_HOST` not set");
    PROMETHEUS_HOST.set(prometheus_host).ok();

    let app = Router::new().route("/api/v1/power-usage", get(power_usage_handler));

    let addr: SocketAddr = "0.0.0.0:9118".parse().unwrap();
    println!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn power_usage_handler(
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    match handle_power_usage(params).await {
        Ok(response) => response.into_response(),
        Err(code) => (code, "Invalid request").into_response(),
    }
}

async fn handle_power_usage(params: HashMap<String, String>) -> Result<axum::response::Response, StatusCode> {
    let target = params
        .get("target")
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_string();

    let date = params
        .get("date")
        .and_then(|d| {
            let parts: Vec<u32> = d.split('-').filter_map(|s| s.parse().ok()).collect();
            if parts.len() == 3 {
                Some((parts[0] as i32, parts[1], parts[2]))
            } else {
                None
            }
        })
        .ok_or(StatusCode::BAD_REQUEST)?;

    let time = params
        .get("time")
        .and_then(|t| {
            let parts: Vec<u32> = t.split(':').filter_map(|s| s.parse().ok()).collect();
            if parts.len() == 2 {
                Some((parts[0], parts[1]))
            } else {
                None
            }
        })
        .ok_or(StatusCode::BAD_REQUEST)?;

    let csv = params.get("csv").map_or(false, |v| v == "true");

    let wib_tz = FixedOffset::east_opt(7 * HOUR).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let naive_date = NaiveDate::from_ymd_opt(date.0, date.1, date.2)
        .and_then(|d| d.and_hms_opt(time.0, time.1, 0))
        .ok_or(StatusCode::BAD_REQUEST)?;

    let curr_dt = naive_date
        .and_local_timezone(wib_tz)
        .single()
        .ok_or(StatusCode::BAD_REQUEST)?
        .with_timezone(&Utc);

    let prev_dt = curr_dt - Duration::days(1);

    let curr_data = get_data(&target, curr_dt).await?;
    let prev_data = get_data(&target, prev_dt).await?;

    let mut result: HashMap<String, Vec<PowerUsage>> = HashMap::new();

    for (key, curr_values) in &curr_data {
        let prev_values = match prev_data.get(key) {
            Some(p) => p,
            None => continue,
        };

        let v: Vec<PowerUsage> = curr_values
            .iter()
            .zip(prev_values.iter())
            .map(|(curr, prev)| {
                let daily = curr - prev;
                PowerUsage {
                    prev_kwh: *prev,
                    curr_kwh: *curr,
                    daily_kwh: daily,
                    avg_power_watt: (daily / 24.0 * 100000.0).round() / 100.0,
                }
            })
            .collect();
        result.insert(key.to_string(), v);
    }

    if csv {
        let mut csv_data = String::new();
        csv_data.push_str("Target,Address,Prev_kWh,Current_kWh,Daily_KWh,Avg_Power_Watt\n");
        for (key, usages) in &result {
            for (i, usage) in usages.iter().enumerate() {
                if usage.avg_power_watt != 0.0 {
                    csv_data.push_str(&format!(
                        "{},{},{},{},{},{}\n",
                        key,
                        i + 1,
                        usage.prev_kwh,
                        usage.curr_kwh,
                        usage.daily_kwh,
                        usage.avg_power_watt
                    ));
                }
            }
        }
        return Ok((StatusCode::OK, csv_data).into_response());
    }

    Ok((StatusCode::OK, Json(result)).into_response())
}

async fn get_data(
    target: &str,
    datetime: DateTime<Utc>,
) -> Result<HashMap<String, Vec<f64>>, StatusCode> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let host = PROMETHEUS_HOST.get().expect("PROMETHEUS_HOST not set");
    let url = format!("http://{}/api/v1/query", host);

    let query_time = datetime.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let query = vec![
        ("query", format!("last_over_time({{__name__=\"energy\",instance=~\"{}\"}}[10m])", target)),
        ("time", query_time),
    ];

    let res: Value = client
        .get(url)
        .query(&query)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?
        .json()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let array = res["data"]["result"]
        .as_array()
        .ok_or(StatusCode::BAD_GATEWAY)?;

    let mut sorted = array.clone();
    sorted.sort_by_key(|item| {
        item["metric"]["address"]
            .as_str()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    });

    let mut result_map = HashMap::new();

    for item in sorted {
        let instance = item["metric"]["instance"].as_str().unwrap_or("unknown");
        let val = item["value"][1].as_str().and_then(|s| s.parse::<f64>().ok());

        if let Some(value) = val {
            result_map
                .entry(instance.to_string())
                .or_insert_with(Vec::new)
                .push(value);
        }
    }

    Ok(result_map)
}
