use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;

#[derive(Debug, Clone)]
struct OpencliPaths {
    node_dir: PathBuf,
    node_exe: Option<PathBuf>,
    script: Option<PathBuf>,
    command: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentStatus {
    ready: bool,
    message: String,
    version: Option<String>,
    browser_detail: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceTestRequest {
    hotel_id: String,
    checkin: String,
    checkout: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotelSearchRequest {
    query: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotelSearchResult {
    pub hotel_id: String,
    pub name: String,
    pub city_name: String,
    pub province_name: String,
    pub display_type: String,
    pub url: String,
    pub source: String,
    pub min_price: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarRequest {
    hotel_id: String,
    start_date: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarPrice {
    date: String,
    price: Option<i64>,
    show_price_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarResult {
    status: String,
    message: String,
    hotel_id: String,
    resolved_hotel_id: String,
    hotel_name: String,
    started_at: String,
    duration_ms: u128,
    prices: Vec<CalendarPrice>,
    min_price: Option<i64>,
    min_dates: Vec<String>,
    signals: Vec<DiagnosticSignal>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticSignal {
    label: String,
    status: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomData {
    sale_room_id: String,
    room_name: String,
    price: i64,
    cancel_policy: String,
    meal: String,
    bed_type: String,
    area: String,
    is_booking: bool,
    remain_rooms: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceTestResult {
    status: String,
    message: String,
    started_at: String,
    duration_ms: u128,
    hotel_url: String,
    rooms: Vec<RoomData>,
    signals: Vec<DiagnosticSignal>,
}

fn signal(label: &str, status: &str, detail: impl Into<String>) -> DiagnosticSignal {
    DiagnosticSignal {
        label: label.to_string(),
        status: status.to_string(),
        detail: detail.into(),
    }
}

fn bundled_candidates(resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(resource_dir) = resource_dir {
        roots.push(resource_dir.join("resources").join("node"));
        roots.push(resource_dir.join("node"));
    }
    roots.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("node"),
    );
    roots
}

fn script_candidates(root: &Path) -> [PathBuf; 2] {
    [
        root.join("opencli-runtime")
            .join("node_modules")
            .join("@jackwener")
            .join("opencli")
            .join("dist")
            .join("src")
            .join("main.js"),
        root.join("node_modules")
            .join("@jackwener")
            .join("opencli")
            .join("dist")
            .join("src")
            .join("main.js"),
    ]
}

fn resolve_opencli(resource_dir: Option<&Path>) -> Result<OpencliPaths, String> {
    for root in bundled_candidates(resource_dir) {
        if !root.join("node.exe").exists() {
            continue;
        }
        for script in script_candidates(&root) {
            if script.exists() {
                return Ok(OpencliPaths {
                    node_dir: root.clone(),
                    node_exe: Some(root.join("node.exe")),
                    script: Some(script),
                    command: None,
                });
            }
        }
    }

    if cfg!(windows) {
        let output = std::process::Command::new("where.exe")
            .arg("opencli.cmd")
            .output()
            .map_err(|error| format!("无法查找 OpenCLI：{error}"))?;
        if output.status.success() {
            if let Some(path) = String::from_utf8_lossy(&output.stdout).lines().next() {
                let command = PathBuf::from(path.trim());
                let node_dir = command.parent().unwrap_or(Path::new(".")).to_path_buf();
                return Ok(OpencliPaths {
                    node_dir,
                    node_exe: None,
                    script: None,
                    command: Some(command),
                });
            }
        }
    }

    Err("未找到 OpenCLI。请重新安装测试台，或先运行 npm run runtime:prepare。".to_string())
}

fn trim_output(value: &str, limit: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(limit).collect()
}

async fn run_opencli_raw(
    paths: &OpencliPaths,
    args: &[&str],
    seconds: u64,
) -> Result<String, String> {
    let current_path = std::env::var("PATH").unwrap_or_default();
    let joined_path = format!("{};{}", paths.node_dir.to_string_lossy(), current_path);
    let child = if let (Some(node_exe), Some(script)) = (&paths.node_exe, &paths.script) {
        let mut command = Command::new(node_exe);
        command.arg(script).args(args);
        command
            .env("PATH", &joined_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.output()
    } else if let Some(opencli_command) = &paths.command {
        let mut command = Command::new("cmd.exe");
        command.args(["/D", "/C"]).arg(opencli_command).args(args);
        command
            .env("PATH", &joined_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.output()
    } else {
        return Err("OpenCLI 启动配置无效。".to_string());
    };

    let output = timeout(Duration::from_secs(seconds), child)
        .await
        .map_err(|_| format!("OpenCLI 执行超过 {seconds} 秒"))?
        .map_err(|error| format!("无法启动 OpenCLI：{error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        let detail = if stderr.trim().is_empty() {
            stdout
        } else {
            format!("{stderr} {stdout}")
        };
        return Err(trim_output(&detail, 600));
    }
    Ok(if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    })
}

fn first_json(value: &str) -> Option<Value> {
    for (index, character) in value.char_indices() {
        if character != '{' && character != '[' {
            continue;
        }
        let mut values = serde_json::Deserializer::from_str(&value[index..]).into_iter::<Value>();
        if let Some(Ok(parsed)) = values.next() {
            return Some(parsed);
        }
    }
    None
}

async fn run_opencli_json(
    paths: &OpencliPaths,
    args: &[&str],
    seconds: u64,
) -> Result<Value, String> {
    let output = run_opencli_raw(paths, args, seconds).await?;
    first_json(&output).ok_or_else(|| format!("OpenCLI 未返回 JSON：{}", trim_output(&output, 240)))
}

fn contains_environment_failure(message: &str) -> bool {
    let normalized = message.to_lowercase();
    [
        "not found",
        "not recognized",
        "enoent",
        "econnrefused",
        "daemon",
        "extension",
        "connect timeout",
        "connection refused",
        "未找到",
        "无法启动",
        "浏览器连接",
        "执行超过",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn contains_restriction_marker(value: &Value) -> Option<&'static str> {
    let normalized = value.to_string().to_lowercase();
    [
        ("captcha", "返回内容包含验证码标记"),
        ("verify", "返回内容要求安全验证"),
        ("forbidden", "返回内容标记为禁止访问"),
        ("risk", "返回内容包含风控标记"),
        ("访问异常", "页面提示访问异常"),
        ("安全验证", "页面要求安全验证"),
        ("操作频繁", "页面提示操作频繁"),
        ("请求频繁", "页面提示请求频繁"),
    ]
    .iter()
    .find_map(|(marker, description)| normalized.contains(marker).then_some(*description))
}

fn find_network_entry(network: &Value, marker: &str) -> Option<(String, Value)> {
    let entries = network.get("entries").unwrap_or(network);
    if let Some(object) = entries.as_object() {
        for (entry_key, entry) in object {
            let url = entry.get("url").and_then(Value::as_str).unwrap_or("");
            if url.to_lowercase().contains(&marker.to_lowercase()) {
                let key = entry
                    .get("key")
                    .and_then(Value::as_str)
                    .unwrap_or(entry_key);
                return Some((key.to_string(), entry.clone()));
            }
        }
    }
    if let Some(array) = entries.as_array() {
        for entry in array {
            let url = entry.get("url").and_then(Value::as_str).unwrap_or("");
            if url.to_lowercase().contains(&marker.to_lowercase()) {
                let key = entry
                    .get("key")
                    .or_else(|| entry.get("id"))
                    .or_else(|| entry.get("url"))?
                    .as_str()?;
                return Some((key.to_string(), entry.clone()));
            }
        }
    }
    None
}

fn find_soa_entry(network: &Value) -> Option<(String, Value)> {
    find_network_entry(network, "gethotelroomlistinland")
}

fn find_top_calendar_control_ref(found: &Value) -> Option<String> {
    let entries = found.get("entries").and_then(Value::as_array)?;
    let is_visible = |entry: &&Value| {
        entry
            .get("visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let entry = entries
        .iter()
        .find(|entry| entry.get("nth").and_then(Value::as_u64) == Some(0) && is_visible(entry))
        .or_else(|| entries.iter().find(is_visible))
        .or_else(|| entries.first())?;
    let reference = value_text(entry.get("ref"));
    (!reference.is_empty()).then_some(reference)
}

fn unwrap_detail(detail: &Value) -> Value {
    let body = detail.get("body").unwrap_or(detail);
    if let Some(body_text) = body.as_str() {
        first_json(body_text).unwrap_or_else(|| Value::String(body_text.to_string()))
    } else {
        body.clone()
    }
}

fn value_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Object(object)) => object
            .get("title")
            .or_else(|| object.get("desc"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn value_i64(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|number| number.round() as i64))
                .or_else(|| value.as_str()?.parse().ok())
        })
        .unwrap_or(0)
}

fn cancellation_text(sale: &Value) -> String {
    let cancel = match sale.get("cancelInfo") {
        Some(cancel) => cancel,
        None => return String::new(),
    };
    value_text(cancel.get("simpleDesc")).or_else_nonempty(|| value_text(cancel.get("title")))
}

trait NonEmptyFallback {
    fn or_else_nonempty(self, fallback: impl FnOnce() -> String) -> String;
}

impl NonEmptyFallback for String {
    fn or_else_nonempty(self, fallback: impl FnOnce() -> String) -> String {
        if self.trim().is_empty() {
            fallback()
        } else {
            self
        }
    }
}

fn parse_rooms(detail: &Value) -> (Vec<RoomData>, bool, usize) {
    let body = unwrap_detail(detail);
    let data = body.get("data").unwrap_or(&body);
    let sale_map = match data.get("saleRoomMap").and_then(Value::as_object) {
        Some(map) => map,
        None => return (Vec::new(), false, 0),
    };
    let physical_map = data.get("physicRoomMap").and_then(Value::as_object);
    let mut rooms = Vec::new();

    for sale in sale_map.values() {
        let hidden = sale
            .get("bookingStatusInfo")
            .and_then(|value| value.get("isHidePrice"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if hidden {
            continue;
        }
        let physical_id = value_text(sale.get("physicalRoomId"));
        let physical = physical_map.and_then(|map| map.get(&physical_id));
        let room_name = physical
            .map(|value| value_text(value.get("name")))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| value_text(sale.get("name")));
        let price = value_i64(
            sale.get("priceInfo")
                .and_then(|value| value.get("price"))
                .or_else(|| sale.get("comparingAmount")),
        );
        if room_name.is_empty() || price <= 0 {
            continue;
        }
        let booking = sale.get("bookingStatusInfo");
        rooms.push(RoomData {
            sale_room_id: value_text(sale.get("id")),
            room_name,
            price,
            cancel_policy: cancellation_text(sale),
            meal: value_text(sale.get("mealInfo").and_then(|value| value.get("title"))),
            bed_type: physical
                .map(|value| value_text(value.get("bedInfo")))
                .unwrap_or_default(),
            area: physical
                .map(|value| value_text(value.get("areaInfo")))
                .unwrap_or_default(),
            is_booking: booking
                .and_then(|value| value.get("isBooking"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            remain_rooms: booking
                .and_then(|value| value.get("remainRoomQuantity"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
        });
    }
    rooms.sort_by_key(|room| room.price);
    (rooms, true, sale_map.len())
}

fn parse_optional_price(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value.round() as i64)),
        Some(Value::String(text)) => {
            let digits = text
                .chars()
                .filter(|character| character.is_ascii_digit())
                .collect::<String>();
            digits.parse::<i64>().ok().filter(|price| *price > 0)
        }
        _ => None,
    }
}

fn normalize_calendar_prices(
    prices: impl IntoIterator<Item = CalendarPrice>,
) -> Vec<CalendarPrice> {
    let mut by_date = BTreeMap::new();
    for price in prices {
        if chrono::NaiveDate::parse_from_str(&price.date, "%Y-%m-%d").is_ok() {
            by_date
                .entry(price.date.clone())
                .and_modify(|existing: &mut CalendarPrice| {
                    if existing.price.is_none()
                        || price
                            .price
                            .is_some_and(|value| value < existing.price.unwrap_or(i64::MAX))
                    {
                        *existing = price.clone();
                    }
                })
                .or_insert(price);
        }
    }
    by_date.into_values().collect()
}

fn parse_calendar_detail(detail: &Value) -> Vec<CalendarPrice> {
    let body = unwrap_detail(detail);
    let data = body.get("data").unwrap_or(&body);
    let entries = match data.get("priceCalendarInfos").and_then(Value::as_array) {
        Some(entries) => entries,
        None => return Vec::new(),
    };
    normalize_calendar_prices(entries.iter().filter_map(|entry| {
        let date = value_text(entry.get("date"));
        if date.is_empty() {
            return None;
        }
        Some(CalendarPrice {
            date,
            price: parse_optional_price(entry.get("minPrice")),
            show_price_type: value_text(entry.get("showPriceType")),
        })
    }))
}

fn parse_calendar_dom(value: &Value) -> (String, String, Vec<CalendarPrice>) {
    let hotel_name = value_text(value.get("hotelName"));
    let resolved_hotel_id = value_text(value.get("resolvedHotelId"));
    let prices = value
        .get("prices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let date = value_text(entry.get("date"));
            if date.is_empty() {
                return None;
            }
            Some(CalendarPrice {
                date,
                price: parse_optional_price(entry.get("price")),
                show_price_type: "页面日历".to_string(),
            })
        });
    (
        hotel_name,
        resolved_hotel_id,
        normalize_calendar_prices(prices),
    )
}

fn parse_hotel_suggestions(value: &Value) -> Vec<HotelSearchResult> {
    let entries = value.as_array().cloned().unwrap_or_default();
    let mut hotels = Vec::new();
    for entry in entries {
        let kind = value_text(entry.get("type"));
        let display_type = value_text(entry.get("displayType"));
        if !kind.eq_ignore_ascii_case("hotel") && display_type != "酒店" {
            continue;
        }
        let hotel_id = value_text(entry.get("id"));
        if hotel_id.is_empty() || !hotel_id.chars().all(|character| character.is_ascii_digit()) {
            continue;
        }
        let name = value_text(entry.get("name"));
        if hotels
            .iter()
            .any(|hotel: &HotelSearchResult| hotel.hotel_id == hotel_id)
        {
            continue;
        }
        hotels.push(HotelSearchResult {
            hotel_id: hotel_id.clone(),
            name: if name.is_empty() {
                format!("酒店 ID {hotel_id}")
            } else {
                name
            },
            city_name: value_text(entry.get("cityName")),
            province_name: value_text(entry.get("provinceName")),
            display_type: if display_type.is_empty() {
                "酒店".to_string()
            } else {
                display_type
            },
            url: value_text(entry.get("url")),
            source: "suggest".to_string(),
            min_price: None,
        });
    }
    hotels
}

fn calendar_summary(prices: &[CalendarPrice]) -> (Option<i64>, Vec<String>) {
    let min_price = prices.iter().filter_map(|item| item.price).min();
    let min_dates = min_price
        .map(|minimum| {
            prices
                .iter()
                .filter(|item| item.price == Some(minimum))
                .map(|item| item.date.clone())
                .collect()
        })
        .unwrap_or_default();
    (min_price, min_dates)
}

fn iso_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn validate_request(request: &PriceTestRequest) -> Result<(), String> {
    if request.hotel_id.is_empty()
        || !request
            .hotel_id
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err("酒店 ID 只能包含数字。".to_string());
    }
    let checkin = chrono::NaiveDate::parse_from_str(&request.checkin, "%Y-%m-%d")
        .map_err(|_| "入住日期格式无效。".to_string())?;
    let checkout = chrono::NaiveDate::parse_from_str(&request.checkout, "%Y-%m-%d")
        .map_err(|_| "离店日期格式无效。".to_string())?;
    if checkout <= checkin {
        return Err("离店日期必须晚于入住日期。".to_string());
    }
    if (checkout - checkin).num_days() > 30 {
        return Err("单次测试的入住区间不能超过 30 天。".to_string());
    }
    Ok(())
}

fn finish(
    status: &str,
    message: impl Into<String>,
    started_at: String,
    timer: Instant,
    hotel_url: String,
    rooms: Vec<RoomData>,
    signals: Vec<DiagnosticSignal>,
) -> PriceTestResult {
    PriceTestResult {
        status: status.to_string(),
        message: message.into(),
        started_at,
        duration_ms: timer.elapsed().as_millis(),
        hotel_url,
        rooms,
        signals,
    }
}

#[tauri::command]
pub async fn check_environment(app: AppHandle) -> EnvironmentStatus {
    let resource_dir = app.path().resource_dir().ok();
    let paths = match resolve_opencli(resource_dir.as_deref()) {
        Ok(paths) => paths,
        Err(message) => {
            return EnvironmentStatus {
                ready: false,
                message,
                version: None,
                browser_detail: None,
            }
        }
    };

    let version_output = match run_opencli_raw(&paths, &["--version"], 15).await {
        Ok(output) => output,
        Err(message) => {
            return EnvironmentStatus {
                ready: false,
                message,
                version: None,
                browser_detail: None,
            }
        }
    };
    let version = trim_output(&version_output, 80);
    match run_opencli_raw(&paths, &["doctor"], 45).await {
        Ok(output) => {
            let summary = trim_output(&output, 220);
            let lower = summary.to_lowercase();
            let disconnected = lower.contains("not connected")
                || lower.contains("unavailable")
                || lower.contains("failed")
                || lower.contains("未连接");
            EnvironmentStatus {
                ready: !disconnected,
                message: if disconnected {
                    "OpenCLI 已安装，但浏览器桥接未就绪。请打开 Chrome 并启用 OpenCLI 扩展。"
                        .to_string()
                } else {
                    "OpenCLI 与浏览器桥接可用，可以执行真实查价。".to_string()
                },
                version: Some(version),
                browser_detail: Some(summary),
            }
        }
        Err(message) => EnvironmentStatus {
            ready: false,
            message: "OpenCLI 已安装，但浏览器桥接未就绪。请打开 Chrome 并启用 OpenCLI 扩展。"
                .to_string(),
            version: Some(version),
            browser_detail: Some(trim_output(&message, 220)),
        },
    }
}

fn extract_hotel_id(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if !trimmed.is_empty() && trimmed.chars().all(|character| character.is_ascii_digit()) {
        return Some(trimmed.to_string());
    }
    let lower = trimmed.to_lowercase();
    let start = lower.find("hotelid=")? + "hotelid=".len();
    let hotel_id = lower[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    (!hotel_id.is_empty()).then_some(hotel_id)
}

#[tauri::command]
pub async fn search_ctrip_hotels(
    app: AppHandle,
    state: State<'_, Mutex<rusqlite::Connection>>,
    request: HotelSearchRequest,
) -> Result<Vec<HotelSearchResult>, String> {
    let query = request.query.trim();
    if query.is_empty() {
        return Err("请输入酒店名称、关键词或酒店 ID。".to_string());
    }
    if query.chars().count() > 80 {
        return Err("搜索内容不能超过 80 个字符。".to_string());
    }
    let hotels = if let Some(hotel_id) = extract_hotel_id(query) {
        vec![HotelSearchResult {
            hotel_id: hotel_id.clone(),
            name: format!("酒店 ID {hotel_id}"),
            city_name: String::new(),
            province_name: String::new(),
            display_type: "ID 直达".to_string(),
            url: format!("https://hotels.ctrip.com/hotels/detail/?hotelid={hotel_id}"),
            source: "id".to_string(),
            min_price: None,
        }]
    } else {
        let resource_dir = app.path().resource_dir().ok();
        let paths = resolve_opencli(resource_dir.as_deref())?;
        let value = run_opencli_json(
            &paths,
            &[
                "ctrip",
                "hotel-suggest",
                query,
                "--limit",
                "20",
                "-f",
                "json",
            ],
            30,
        )
        .await?;
        parse_hotel_suggestions(&value)
    };

    let conn = state.lock().await;
    crate::storage::upsert_hotels(&conn, &hotels)?;
    let stored = crate::storage::load_hotels(&conn)?;
    let price_map: HashMap<&str, i64> = stored
        .iter()
        .filter_map(|hotel| {
            hotel
                .min_price
                .map(|price| (hotel.hotel_id.as_str(), price))
        })
        .collect();
    Ok(hotels
        .into_iter()
        .map(|mut hotel| {
            hotel.min_price = price_map.get(hotel.hotel_id.as_str()).copied();
            hotel
        })
        .collect())
}

fn finish_calendar(
    status: &str,
    message: impl Into<String>,
    request: &CalendarRequest,
    resolved_hotel_id: String,
    hotel_name: String,
    started_at: String,
    timer: Instant,
    prices: Vec<CalendarPrice>,
    signals: Vec<DiagnosticSignal>,
) -> CalendarResult {
    let (min_price, min_dates) = calendar_summary(&prices);
    CalendarResult {
        status: status.to_string(),
        message: message.into(),
        hotel_id: request.hotel_id.clone(),
        resolved_hotel_id,
        hotel_name,
        started_at,
        duration_ms: timer.elapsed().as_millis(),
        prices,
        min_price,
        min_dates,
        signals,
    }
}

#[tauri::command]
pub async fn get_ctrip_price_calendar(
    app: AppHandle,
    request: CalendarRequest,
) -> Result<CalendarResult, String> {
    if request.hotel_id.is_empty()
        || !request
            .hotel_id
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err("酒店 ID 只能包含数字。".to_string());
    }
    let checkin = chrono::NaiveDate::parse_from_str(&request.start_date, "%Y-%m-%d")
        .map_err(|_| "日历起始日期格式无效。".to_string())?;
    let checkout = checkin + chrono::Duration::days(1);
    let timer = Instant::now();
    let started_at = iso_now();
    let resource_dir = app.path().resource_dir().ok();
    let paths = resolve_opencli(resource_dir.as_deref())?;
    let hotel_url = format!(
        "https://hotels.ctrip.com/hotels/detail/?hotelid={}&checkin={}&checkout={}",
        request.hotel_id,
        checkin.format("%Y-%m-%d"),
        checkout.format("%Y-%m-%d")
    );
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let session = format!("ctrip_calendar_{}_{}", std::process::id(), timestamp);
    let mut signals = vec![signal(
        "OpenCLI 运行环境",
        "success",
        "已使用独立版内置 OpenCLI 运行时。",
    )];

    if let Err(message) = run_opencli_json(
        &paths,
        &[
            "browser",
            &session,
            "open",
            &hotel_url,
            "--window",
            "background",
        ],
        45,
    )
    .await
    {
        signals.push(signal("携程详情页", "error", &message));
        let _ = run_opencli_raw(&paths, &["browser", &session, "close"], 10).await;
        return Ok(finish_calendar(
            if contains_environment_failure(&message) {
                "environment_error"
            } else {
                "restricted"
            },
            "酒店详情页未能正常打开。",
            &request,
            request.hotel_id.clone(),
            String::new(),
            started_at,
            timer,
            Vec::new(),
            signals,
        ));
    }
    signals.push(signal("携程详情页", "success", "已在后台打开酒店详情页。"));
    let _ = run_opencli_raw(&paths, &["browser", &session, "wait", "time", "6"], 25).await;

    let mut calendar_opened = false;
    for css in [
        "#checkInInput",
        "#checkInDateInput",
        "input[placeholder*=\"入住\"]",
    ] {
        if let Ok(found) = run_opencli_json(
            &paths,
            &["browser", &session, "find", "--css", css, "--limit", "5"],
            20,
        )
        .await
        {
            let ref_id = find_top_calendar_control_ref(&found);
            if let Some(ref_id) = ref_id.filter(|value| !value.is_empty()) {
                if run_opencli_json(&paths, &["browser", &session, "click", &ref_id], 20)
                    .await
                    .is_ok()
                {
                    calendar_opened = true;
                    let _ =
                        run_opencli_raw(&paths, &["browser", &session, "wait", "time", "2"], 15)
                            .await;
                    break;
                }
            }
        }
    }
    signals.push(if calendar_opened {
        signal("价格日历", "success", "已打开页面顶部搜索栏的价格日历。")
    } else {
        signal(
            "价格日历",
            "warning",
            "未定位到价格日历控件，将尝试读取已加载数据。",
        )
    });

    let mut api_prices = Vec::new();
    let mut calendar_key: Option<String> = None;
    for _ in 0..4 {
        if let Ok(network) = run_opencli_json(&paths, &["browser", &session, "network"], 40).await {
            if let Some((key, _)) = find_network_entry(&network, "gethotelpricecalendar") {
                calendar_key = Some(key);
                break;
            }
        }
        let _ = run_opencli_raw(&paths, &["browser", &session, "wait", "time", "3"], 15).await;
    }
    if let Some(key) = calendar_key {
        signals.push(signal(
            "日历接口",
            "success",
            "已捕获价格日历接口，一次读取完整价格区间。",
        ));
        if let Ok(detail) = run_opencli_json(
            &paths,
            &["browser", &session, "network", "--detail", &key],
            40,
        )
        .await
        {
            api_prices = parse_calendar_detail(&detail);
        }
    }

    const CALENDAR_DOM_SCRIPT: &str = r#"(() => {
      const title = (document.querySelector('h1')?.textContent || document.title.replace(/预订.*/, '') || '').trim();
      const resolvedHotelId = new URL(location.href).searchParams.get('hotelid') || '';
      const isVisible = element => !!(element && (element.offsetWidth || element.offsetHeight) && getComputedStyle(element).visibility !== 'hidden');
      const calendarRoot = [...document.querySelectorAll('[role="application"]')]
        .find(element => isVisible(element) && element.querySelector('[data-d]'))
        || [...document.querySelectorAll('.c-calendar')]
          .find(element => isVisible(element) && element.querySelector('[data-d]'));
      const cells = calendarRoot ? calendarRoot.querySelectorAll('[data-d]') : document.querySelectorAll('[data-d]');
      const prices = [...cells].map(cell => {
        const label = cell.querySelector('.tipWrapper')?.getAttribute('aria-label') || '';
        const match = label.match(/(\d{4})年(\d{1,2})月(\d{1,2})日.*?(?:CNY\s*([\d,]+))?/);
        if (!match) return null;
        const date = `${match[1]}-${match[2].padStart(2, '0')}-${match[3].padStart(2, '0')}`;
        const priceText = cell.querySelector('.price')?.textContent?.trim() || match[4] || '';
        const price = Number(priceText.replace(/[^\d]/g, '')) || null;
        return { date, price };
      }).filter(Boolean);
      return { hotelName: title, resolvedHotelId, prices };
    })()"#;
    let (hotel_name, resolved_hotel_id, dom_prices) = match run_opencli_json(
        &paths,
        &["browser", &session, "eval", CALENDAR_DOM_SCRIPT],
        25,
    )
    .await
    {
        Ok(value) => parse_calendar_dom(&value),
        Err(_) => (String::new(), String::new(), Vec::new()),
    };
    let _ = run_opencli_raw(&paths, &["browser", &session, "close"], 15).await;

    let prices = normalize_calendar_prices(api_prices.into_iter().chain(dom_prices));
    let resolved_hotel_id = if resolved_hotel_id.is_empty() {
        request.hotel_id.clone()
    } else {
        resolved_hotel_id
    };
    if prices.is_empty() {
        signals.push(signal(
            "价格解析",
            "warning",
            "页面与日历接口均未返回日期价格。",
        ));
        return Ok(finish_calendar(
            "restricted",
            "没有读取到价格日历，可能是页面加载、风控验证或接口变化。",
            &request,
            resolved_hotel_id,
            hotel_name,
            started_at,
            timer,
            prices,
            signals,
        ));
    }
    let priced_days = prices.iter().filter(|item| item.price.is_some()).count();
    if priced_days == 0 {
        signals.push(signal(
            "价格解析",
            "neutral",
            format!("读取到 {} 天，但没有可售价格。", prices.len()),
        ));
        Ok(finish_calendar(
            "no_inventory",
            "日历接口返回正常，但当前区间没有可售价格；这不等同于封号。",
            &request,
            resolved_hotel_id,
            hotel_name,
            started_at,
            timer,
            prices,
            signals,
        ))
    } else {
        signals.push(signal(
            "价格解析",
            "success",
            format!(
                "已读取 {} 天，其中 {} 天有可售最低价。",
                prices.len(),
                priced_days
            ),
        ));
        Ok(finish_calendar(
            "success",
            "酒店价格日历读取成功，已标出整个区间的最低价。",
            &request,
            resolved_hotel_id,
            hotel_name,
            started_at,
            timer,
            prices,
            signals,
        ))
    }
}

#[tauri::command]
pub async fn test_ctrip_price(
    app: AppHandle,
    request: PriceTestRequest,
) -> Result<PriceTestResult, String> {
    validate_request(&request)?;
    let timer = Instant::now();
    let started_at = iso_now();
    let hotel_url = format!(
        "https://hotels.ctrip.com/hotels/detail/?hotelid={}&checkin={}&checkout={}",
        request.hotel_id, request.checkin, request.checkout
    );
    let resource_dir = app.path().resource_dir().ok();
    let paths = match resolve_opencli(resource_dir.as_deref()) {
        Ok(paths) => paths,
        Err(message) => {
            return Ok(finish(
                "environment_error",
                message.clone(),
                started_at,
                timer,
                hotel_url,
                Vec::new(),
                vec![signal("OpenCLI 运行环境", "error", message)],
            ))
        }
    };
    let mut signals = vec![signal(
        "OpenCLI 运行环境",
        "success",
        "已找到独立版内置 OpenCLI 运行时。",
    )];
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let session = format!("ctrip_price_test_{}_{}", std::process::id(), timestamp);

    if let Err(message) = run_opencli_json(
        &paths,
        &[
            "browser",
            &session,
            "open",
            &hotel_url,
            "--window",
            "background",
        ],
        45,
    )
    .await
    {
        signals.push(signal("携程详情页", "error", &message));
        let status = if contains_environment_failure(&message) {
            "environment_error"
        } else {
            "restricted"
        };
        let summary = if status == "environment_error" {
            "浏览器或 OpenCLI 未能启动，请先处理运行环境。"
        } else {
            "携程详情页未能正常打开，当前机器可能受到访问限制。"
        };
        let _ = run_opencli_raw(&paths, &["browser", &session, "close"], 10).await;
        return Ok(finish(
            status,
            summary,
            started_at,
            timer,
            hotel_url,
            Vec::new(),
            signals,
        ));
    }
    signals.push(signal(
        "携程详情页",
        "success",
        "后台浏览器已打开指定酒店与日期。",
    ));

    let _ = run_opencli_raw(&paths, &["browser", &session, "wait", "time", "6"], 25).await;
    let network_result = run_opencli_json(&paths, &["browser", &session, "network"], 40).await;
    let result = match network_result {
        Err(message) => {
            signals.push(signal("房价接口", "error", &message));
            finish(
                "environment_error",
                "无法读取浏览器网络记录，请检查 OpenCLI 扩展连接。",
                started_at.clone(),
                timer,
                hotel_url.clone(),
                Vec::new(),
                signals,
            )
        }
        Ok(network) => {
            match find_soa_entry(&network) {
                None => {
                    signals.push(signal("房价接口", "warning", "未捕获 getHotelRoomListInland。可能是页面风控、加载失败或当前页面结构变化。"));
                    finish(
                        "restricted",
                        "页面已打开，但没有出现携程房价接口，建议在正常机器上用相同条件复测。",
                        started_at.clone(),
                        timer,
                        hotel_url.clone(),
                        Vec::new(),
                        signals,
                    )
                }
                Some((key, _entry)) => {
                    signals.push(signal(
                        "房价接口",
                        "success",
                        "已捕获 getHotelRoomListInland 网络请求。",
                    ));
                    match run_opencli_json(
                        &paths,
                        &["browser", &session, "network", "--detail", &key],
                        40,
                    )
                    .await
                    {
                        Err(message) => {
                            signals.push(signal("结果解析", "warning", &message));
                            finish(
                                "restricted",
                                "已捕获房价接口，但响应详情无法读取，可能受到访问限制。",
                                started_at.clone(),
                                timer,
                                hotel_url.clone(),
                                Vec::new(),
                                signals,
                            )
                        }
                        Ok(detail) => {
                            let (rooms, has_sale_map, sale_count) = parse_rooms(&detail);
                            if !rooms.is_empty() {
                                signals.push(signal(
                                    "结果解析",
                                    "success",
                                    format!("成功解析 {} 条可售房型。", rooms.len()),
                                ));
                                finish(
                                    "success",
                                    "房价接口与房型价格均正常返回，本次未发现封禁或风控迹象。",
                                    started_at.clone(),
                                    timer,
                                    hotel_url.clone(),
                                    rooms,
                                    signals,
                                )
                            } else if has_sale_map {
                                signals.push(signal(
                                    "结果解析",
                                    "neutral",
                                    format!(
                                        "接口正常返回，共 {} 条报价记录，但没有可售价格。",
                                        sale_count
                                    ),
                                ));
                                finish("no_inventory", "房价接口返回正常，但当前酒店与日期没有可售房型；这不等同于封号。", started_at.clone(), timer, hotel_url.clone(), Vec::new(), signals)
                            } else {
                                let marker = contains_restriction_marker(&unwrap_detail(&detail))
                                    .unwrap_or("响应中缺少 saleRoomMap 房价结构");
                                signals.push(signal("结果解析", "warning", marker));
                                finish(
                                    "restricted",
                                    "房价接口响应结构异常，存在风控、验证页或接口变更的可能。",
                                    started_at.clone(),
                                    timer,
                                    hotel_url.clone(),
                                    Vec::new(),
                                    signals,
                                )
                            }
                        }
                    }
                }
            }
        }
    };

    let _ = run_opencli_raw(&paths, &["browser", &session, "close"], 15).await;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_ctrip_room_list_network_entry() {
        let network = json!({
            "entries": {
                "request-1": { "url": "https://example.test/other" },
                "request-2": { "url": "https://m.ctrip.com/restapi/soa2/getHotelRoomListInland" }
            }
        });
        let (key, _) = find_soa_entry(&network).expect("room list entry");
        assert_eq!(key, "request-2");
    }

    #[test]
    fn matches_calendar_entry_with_or_without_ct_prefix() {
        let network = json!({
            "entries": {
                "request-1": { "url": "https://hotels.ctrip.com/restapi/soa2/13389/json/GetHotelPriceCalendar" },
                "request-2": { "url": "https://m.ctrip.com/restapi/soa2/ctGetHotelPriceCalendar" }
            }
        });
        let (key, _) =
            find_network_entry(&network, "gethotelpricecalendar").expect("calendar entry");
        assert_eq!(key, "request-1");
    }

    #[test]
    fn selects_top_search_calendar_control_instead_of_room_section() {
        let found = json!({
            "entries": [
                { "nth": 0, "ref": 17, "visible": true, "attrs": { "id": "checkInInput" } },
                { "nth": 1, "ref": 42, "visible": true, "attrs": { "id": "checkInInput" } }
            ]
        });
        assert_eq!(find_top_calendar_control_ref(&found).as_deref(), Some("17"));
    }

    #[test]
    fn falls_back_to_first_visible_calendar_control() {
        let found = json!({
            "entries": [
                { "nth": 0, "ref": 17, "visible": false },
                { "nth": 1, "ref": 42, "visible": true }
            ]
        });
        assert_eq!(find_top_calendar_control_ref(&found).as_deref(), Some("42"));
    }

    #[test]
    fn parses_and_sorts_room_prices() {
        let detail = json!({
            "body": {
                "data": {
                    "saleRoomMap": {
                        "sale-a": {
                            "id": "sale-a",
                            "physicalRoomId": 1,
                            "priceInfo": { "price": 488 },
                            "mealInfo": { "title": "双早" },
                            "cancelInfo": { "simpleDesc": "入住前一天可免费取消" },
                            "bookingStatusInfo": { "isBooking": true, "remainRoomQuantity": 2 }
                        },
                        "sale-b": {
                            "id": "sale-b",
                            "physicalRoomId": 2,
                            "priceInfo": { "price": 366 },
                            "bookingStatusInfo": { "isBooking": true }
                        }
                    },
                    "physicRoomMap": {
                        "1": { "name": "豪华大床房", "bedInfo": { "title": "1张大床" }, "areaInfo": { "title": "35㎡" } },
                        "2": { "name": "高级双床房" }
                    }
                }
            }
        });
        let (rooms, has_sale_map, sale_count) = parse_rooms(&detail);
        assert!(has_sale_map);
        assert_eq!(sale_count, 2);
        assert_eq!(rooms.len(), 2);
        assert_eq!(rooms[0].room_name, "高级双床房");
        assert_eq!(rooms[0].price, 366);
        assert_eq!(rooms[1].bed_type, "1张大床");
    }

    #[test]
    fn distinguishes_missing_price_structure() {
        let (rooms, has_sale_map, sale_count) = parse_rooms(&json!({ "body": { "data": {} } }));
        assert!(rooms.is_empty());
        assert!(!has_sale_map);
        assert_eq!(sale_count, 0);
    }

    #[test]
    fn rejects_invalid_test_ranges() {
        let request = PriceTestRequest {
            hotel_id: "488929".to_string(),
            checkin: "2026-08-20".to_string(),
            checkout: "2026-08-20".to_string(),
        };
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn supports_numeric_id_and_ctrip_url_search() {
        assert_eq!(extract_hotel_id("375539").as_deref(), Some("375539"));
        assert_eq!(
            extract_hotel_id(
                "https://hotels.ctrip.com/hotels/detail/?hotelid=375539&checkIn=2026-08-18"
            )
            .as_deref(),
            Some("375539")
        );
        assert!(extract_hotel_id("上海和平饭店").is_none());
    }

    #[test]
    fn keeps_only_hotel_suggestions_in_original_rank_order() {
        let value = json!([
            { "id": "2", "type": "City", "displayType": "城市", "name": "上海" },
            { "id": "375539", "type": "Hotel", "displayType": "酒店", "name": "上海和平饭店", "cityName": "上海" },
            { "id": "123", "type": "Markland", "displayType": "地标", "name": "外滩" },
            { "id": "488929", "type": "Hotel", "displayType": "酒店", "name": "测试酒店" }
        ]);
        let hotels = parse_hotel_suggestions(&value);
        assert_eq!(hotels.len(), 2);
        assert_eq!(hotels[0].hotel_id, "375539");
        assert_eq!(hotels[1].hotel_id, "488929");
    }

    #[test]
    fn parses_calendar_prices_and_finds_minimum_dates() {
        let detail = json!({
            "body": {
                "data": {
                    "priceCalendarInfos": [
                        { "date": "2026-08-18", "minPrice": "2,493", "showPriceType": "CNY" },
                        { "date": "2026-08-19", "minPrice": "2401", "showPriceType": "CNY" },
                        { "date": "2026-08-20", "minPrice": "¥2,401", "showPriceType": "CNY" },
                        { "date": "2026-08-21", "minPrice": "", "showPriceType": "" }
                    ]
                }
            }
        });
        let prices = parse_calendar_detail(&detail);
        assert_eq!(prices.len(), 4);
        assert_eq!(prices[0].price, Some(2493));
        assert_eq!(prices[3].price, None);
        let (minimum, dates) = calendar_summary(&prices);
        assert_eq!(minimum, Some(2401));
        assert_eq!(dates, vec!["2026-08-19", "2026-08-20"]);
    }
}
