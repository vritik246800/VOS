//! Weather module (Phase 6.5): a **multi-city** weather panel backed by
//! open-meteo.com (no API key required).
//!
//! The user keeps a list of cities (persisted in `app.db`). Cities are added by
//! name via a **live search** over Open-Meteo's free geocoding API, which returns
//! multiple candidates refined as the user types. Each city fetches its own
//! current conditions + 24h forecast on a background `std::thread` (the project's
//! "thread + channel" pattern), and the results are cached for 15 minutes in
//! `data/weather_cache.json` (a map keyed by coordinates) so re-opening the panel
//! is instant.
//!
//! Nothing here panics — network/parse failures become `Err(String)` carrying a
//! UI-friendly message.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache file path, relative to CWD (the app always runs from the repo root).
const CACHE_PATH: &str = "data/weather_cache.json";

/// Suggested cities (used to seed an empty list on first run). `(name, lat, lon)`.
pub const CITY_PRESETS: &[(&str, f64, f64)] = &[
    ("Lisbon", 38.7223, -9.1393),
    ("London", 51.5074, -0.1278),
    ("New York", 40.7128, -74.0060),
    ("Tokyo", 35.6762, 139.6503),
    ("Sydney", -33.8688, 151.2093),
    ("Berlin", 52.5200, 13.4050),
];

/// Current load state of a single city.
#[derive(Debug, Clone, PartialEq)]
pub enum WeatherStatus {
    Idle,
    Loading,
    Loaded,
    Error(String),
}

/// One row of the multi-day forecast. Temperatures are always °C (as fetched);
/// the renderer converts to the display unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastDay {
    pub code: u32,
    pub high: f64,
    pub low: f64,
    /// Formatted date, e.g. "25 July, 2026".
    pub date: String,
    pub weekday: String,
}

/// A snapshot of weather data, serialisable to the JSON cache.
///
/// Everything added for the dashboard view is `#[serde(default)]` so caches
/// written by older builds still parse (they just render as zeros until the
/// 15-minute freshness window expires).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherData {
    /// Current temperature, °C.
    pub temp: f64,
    /// WMO weather interpretation code.
    pub weather_code: u32,
    /// Next 24h temperatures, °C.
    pub hourly: Vec<f64>,
    /// Unix seconds at which this data was fetched.
    pub fetched_at: u64,
    /// Apparent ("feels like") temperature, °C.
    #[serde(default)]
    pub feels_like: f64,
    /// Relative humidity, %.
    #[serde(default)]
    pub humidity: u32,
    /// Wind speed, km/h.
    #[serde(default)]
    pub wind_speed: f64,
    /// Local date + time of the observation, e.g. "11 August, 2026  3:05 PM".
    #[serde(default)]
    pub datetime: String,
    /// Local time of the observation, e.g. "3:05 PM".
    #[serde(default)]
    pub time_short: String,
    /// The next days (today excluded).
    #[serde(default)]
    pub forecast: Vec<ForecastDay>,
    /// Precipitation probability for the current hour, %.
    #[serde(default)]
    pub rain_chance: u8,
    /// Next 24h wind speeds, km/h.
    #[serde(default)]
    pub wind_24h: Vec<f64>,
    /// Next 24h precipitation probabilities, %.
    #[serde(default)]
    pub rain_24h: Vec<f64>,
    /// UV index for the current hour (scale 0..=11).
    #[serde(default)]
    pub uv_index: f64,
    /// Local sunrise time, e.g. "6:42 AM".
    #[serde(default)]
    pub sunrise: String,
    /// Local sunset time.
    #[serde(default)]
    pub sunset: String,
    /// Sun position along the day arc, 0.0 (sunrise) ..= 1.0 (sunset).
    #[serde(default)]
    pub sun_frac: f64,
    /// Visibility for the current hour, km.
    #[serde(default)]
    pub visibility_km: f64,
}

/// One saved city with its own fetch state.
pub struct WeatherCity {
    /// Database id (primary key in `weather_cities`).
    pub id: i64,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub status: WeatherStatus,
    pub data: Option<WeatherData>,
    /// Receiver for this city's in-flight background fetch (if any).
    rx: Option<Receiver<Result<WeatherData, String>>>,
}

/// A geocoding search candidate (one possible match for a typed query).
#[derive(Debug, Clone)]
pub struct Candidate {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
}

/// State of the live geocoding search.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchStatus {
    Idle,
    Searching,
    Done,
    Error(String),
}

/// Live city search: a query refined as the user types, resolving to a list of
/// candidate cities on a background thread (latest query wins).
pub struct CitySearch {
    pub active: bool,
    pub query: String,
    pub candidates: Vec<Candidate>,
    pub selected: usize,
    pub status: SearchStatus,
    /// Receiver for the in-flight geocoding lookup (if any).
    rx: Option<Receiver<Result<Vec<Candidate>, String>>>,
    /// The last query that triggered a search (to avoid redundant re-searches).
    last_query: String,
}

impl Default for CitySearch {
    fn default() -> Self {
        Self {
            active: false,
            query: String::new(),
            candidates: Vec::new(),
            selected: 0,
            status: SearchStatus::Idle,
            rx: None,
            last_query: String::new(),
        }
    }
}

/// Temperature display unit. Data is always stored in °C (as fetched); the unit
/// only affects rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}

impl TempUnit {
    pub fn toggle(self) -> Self {
        match self {
            TempUnit::Celsius => TempUnit::Fahrenheit,
            TempUnit::Fahrenheit => TempUnit::Celsius,
        }
    }

    /// Unit suffix for display, e.g. "°C" / "°F".
    pub fn label(self) -> &'static str {
        match self {
            TempUnit::Celsius => "°C",
            TempUnit::Fahrenheit => "°F",
        }
    }

    /// Convert a Celsius value into this unit.
    pub fn convert(self, celsius: f64) -> f64 {
        match self {
            TempUnit::Celsius => celsius,
            TempUnit::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
        }
    }
}

pub struct WeatherPanel {
    pub cities: Vec<WeatherCity>,
    pub selected: usize,
    pub search: CitySearch,
    /// Temperature display unit (does not change the stored/cached °C data).
    pub unit: TempUnit,
    /// Monotonic animation counter, advanced once per UI tick while the Weather
    /// view is open; drives the animated condition art.
    pub anim_tick: u64,
}

impl Default for WeatherPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl WeatherPanel {
    pub fn new() -> Self {
        Self {
            cities: Vec::new(),
            selected: 0,
            search: CitySearch::default(),
            unit: TempUnit::Celsius,
            anim_tick: 0,
        }
    }

    /// Flip the temperature display unit (°C ↔ °F).
    pub fn toggle_unit(&mut self) {
        self.unit = self.unit.toggle();
    }

    /// Replace the city list (e.g. loaded from the DB). Keeps the selection in
    /// bounds. Does not fetch — call [`refresh_all`](Self::refresh_all).
    pub fn set_cities(&mut self, rows: Vec<(i64, String, f64, f64)>) {
        self.cities = rows
            .into_iter()
            .map(|(id, name, lat, lon)| WeatherCity {
                id,
                name,
                lat,
                lon,
                status: WeatherStatus::Idle,
                data: None,
                rx: None,
            })
            .collect();
        if self.selected >= self.cities.len() {
            self.selected = self.cities.len().saturating_sub(1);
        }
    }

    /// Append a city, select it and start fetching its weather.
    pub fn add_city(&mut self, id: i64, name: String, lat: f64, lon: f64) {
        self.cities.push(WeatherCity {
            id,
            name,
            lat,
            lon,
            status: WeatherStatus::Idle,
            data: None,
            rx: None,
        });
        self.selected = self.cities.len() - 1;
        let idx = self.selected;
        self.refresh_city(idx, true);
    }

    /// Whether a city with (approximately) these coordinates already exists.
    pub fn has_city(&self, lat: f64, lon: f64) -> bool {
        self.cities
            .iter()
            .any(|c| (c.lat - lat).abs() < 1e-3 && (c.lon - lon).abs() < 1e-3)
    }

    /// Remove the selected city, keeping the selection in bounds.
    pub fn remove_selected(&mut self) {
        if self.cities.is_empty() {
            return;
        }
        self.cities.remove(self.selected);
        if self.selected >= self.cities.len() {
            self.selected = self.cities.len().saturating_sub(1);
        }
    }

    pub fn selected_city(&self) -> Option<&WeatherCity> {
        self.cities.get(self.selected)
    }

    pub fn selected_city_id(&self) -> Option<i64> {
        self.cities.get(self.selected).map(|c| c.id)
    }

    /// The selected city's weather data (used e.g. for the calendar header).
    pub fn primary_data(&self) -> Option<&WeatherData> {
        self.cities.get(self.selected).and_then(|c| c.data.as_ref())
    }

    pub fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn select_down(&mut self) {
        if self.selected + 1 < self.cities.len() {
            self.selected += 1;
        }
    }

    /// Refresh every city (cache-aware).
    pub fn refresh_all(&mut self) {
        for i in 0..self.cities.len() {
            self.refresh_city(i, false);
        }
    }

    pub fn refresh_selected(&mut self) {
        let idx = self.selected;
        if idx < self.cities.len() {
            self.refresh_city(idx, false);
        }
    }

    pub fn force_refresh_selected(&mut self) {
        let idx = self.selected;
        if idx < self.cities.len() {
            self.refresh_city(idx, true);
        }
    }

    /// Refresh one city: use a fresh cache entry unless `force`, otherwise spawn
    /// a background fetch.
    fn refresh_city(&mut self, idx: usize, force: bool) {
        let Some(city) = self.cities.get_mut(idx) else {
            return;
        };
        if city.status == WeatherStatus::Loading {
            return;
        }
        let key = coord_key(city.lat, city.lon);
        if !force {
            if let Some(cached) = read_cache_map().and_then(|m| m.get(&key).cloned()) {
                if cache_is_fresh(cached.fetched_at, now_unix()) {
                    city.data = Some(cached);
                    city.status = WeatherStatus::Loaded;
                    return;
                }
            }
        }
        let (tx, rx) = mpsc::channel();
        let (lat, lon) = (city.lat, city.lon);
        std::thread::spawn(move || {
            let _ = tx.send(fetch_weather(lat, lon));
        });
        city.rx = Some(rx);
        city.status = WeatherStatus::Loading;
    }

    /// Drain every city's fetch channel; update data/status and the cache.
    pub fn tick(&mut self) {
        for city in &mut self.cities {
            let Some(rx) = &city.rx else { continue };
            match rx.try_recv() {
                Ok(msg) => {
                    city.rx = None;
                    match msg {
                        Ok(data) => {
                            write_cache_entry(&coord_key(city.lat, city.lon), &data);
                            city.data = Some(data);
                            city.status = WeatherStatus::Loaded;
                        }
                        Err(e) => city.status = WeatherStatus::Error(e),
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    city.rx = None;
                    if city.status == WeatherStatus::Loading {
                        city.status = WeatherStatus::Error("fetch failed".to_string());
                    }
                }
            }
        }
    }

    /// True while any city is fetching or a search lookup is in flight (keeps the
    /// tick loop alive).
    pub fn is_loading(&self) -> bool {
        self.search.status == SearchStatus::Searching
            || self
                .cities
                .iter()
                .any(|c| c.status == WeatherStatus::Loading)
    }

    // ── live search ─────────────────────────────────────────────────────────

    /// Open the live search: activate, clear state.
    pub fn search_open(&mut self) {
        self.search.active = true;
        self.search.query.clear();
        self.search.candidates.clear();
        self.search.selected = 0;
        self.search.status = SearchStatus::Idle;
        self.search.rx = None;
        self.search.last_query.clear();
    }

    /// Close the live search: deactivate, clear state.
    pub fn search_close(&mut self) {
        self.search.active = false;
        self.search.query.clear();
        self.search.candidates.clear();
        self.search.selected = 0;
        self.search.status = SearchStatus::Idle;
        self.search.rx = None;
        self.search.last_query.clear();
    }

    /// Append a character to the query; re-search if it now qualifies.
    pub fn search_push(&mut self, c: char) {
        self.search.query.push(c);
        self.maybe_search();
    }

    /// Remove the last character from the query; re-search if it still qualifies.
    pub fn search_pop(&mut self) {
        self.search.query.pop();
        self.maybe_search();
    }

    /// Decide whether the current query warrants a new background geocode.
    fn maybe_search(&mut self) {
        let query = self.search.query.clone();
        if should_search(&query) {
            if query != self.search.last_query {
                let q = query.trim().to_string();
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(geocode_search(&q));
                });
                self.search.rx = Some(rx);
                self.search.status = SearchStatus::Searching;
                self.search.last_query = query;
            }
        } else {
            self.search.candidates.clear();
            self.search.selected = 0;
            self.search.status = SearchStatus::Idle;
            self.search.rx = None;
            self.search.last_query.clear();
        }
    }

    pub fn search_up(&mut self) {
        if self.search.selected > 0 {
            self.search.selected -= 1;
        }
    }

    pub fn search_down(&mut self) {
        if self.search.selected + 1 < self.search.candidates.len() {
            self.search.selected += 1;
        }
    }

    pub fn selected_candidate(&self) -> Option<&Candidate> {
        self.search.candidates.get(self.search.selected)
    }

    /// Drain the geocoding channel, if a result is ready, into the candidate list.
    pub fn tick_search(&mut self) {
        let Some(rx) = &self.search.rx else { return };
        match rx.try_recv() {
            Ok(Ok(list)) => {
                self.search.rx = None;
                let empty = list.is_empty();
                self.search.candidates = list;
                self.search.selected = 0;
                self.search.status = if empty {
                    SearchStatus::Idle
                } else {
                    SearchStatus::Done
                };
            }
            Ok(Err(e)) => {
                self.search.rx = None;
                self.search.candidates.clear();
                self.search.selected = 0;
                self.search.status = SearchStatus::Error(e);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.search.rx = None;
                self.search.status = SearchStatus::Error("search failed".to_string());
            }
        }
    }
}

// ── Network (blocking, run on background threads) ───────────────────────────

/// Blocking HTTP fetch + parse of current conditions, the next 24h hourly
/// series and the multi-day daily forecast.
fn fetch_weather(lat: f64, lon: f64) -> Result<WeatherData, String> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
&current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m\
&hourly=temperature_2m,wind_speed_10m,precipitation_probability,uv_index,visibility\
&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset\
&forecast_days=7&timezone=auto"
    );

    let resp = reqwest::blocking::get(&url).map_err(|e| format!("network error: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("server returned {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .map_err(|e| format!("could not parse response: {e}"))?;

    let current = &json["current"];
    let temp = current["temperature_2m"]
        .as_f64()
        .ok_or_else(|| "missing current temperature".to_string())?;
    let weather_code = current["weather_code"].as_u64().unwrap_or(0) as u32;
    let feels_like = current["apparent_temperature"].as_f64().unwrap_or(0.0);
    let humidity = current["relative_humidity_2m"].as_u64().unwrap_or(0) as u32;
    let wind_speed = current["wind_speed_10m"].as_f64().unwrap_or(0.0);
    let now_iso = current["time"].as_str().unwrap_or("");

    // The hourly arrays start at local midnight, so the current hour indexes
    // straight into them.
    let hour = now_iso
        .get(11..13)
        .and_then(|h| h.parse::<usize>().ok())
        .unwrap_or(12)
        .min(23);
    let series = |key: &str| -> Vec<f64> {
        json["hourly"][key]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|v| v.as_f64().unwrap_or(0.0))
                    .take(24)
                    .collect()
            })
            .unwrap_or_default()
    };
    let hourly = series("temperature_2m");
    let wind_24h = series("wind_speed_10m");
    let rain_24h = series("precipitation_probability");
    let at_hour = |vals: &[f64]| vals.get(hour).copied().unwrap_or(0.0);

    let daily = &json["daily"];
    let day_f64 = |key: &str, i: usize| daily[key][i].as_f64().unwrap_or(0.0);
    let day_str = |key: &str, i: usize| daily[key][i].as_str().unwrap_or("").to_string();
    let days = daily["time"].as_array().map(|a| a.len()).unwrap_or(0);
    let forecast = (1..days.min(5))
        .map(|i| {
            let date = day_str("time", i);
            ForecastDay {
                code: daily["weather_code"][i].as_u64().unwrap_or(0) as u32,
                high: day_f64("temperature_2m_max", i),
                low: day_f64("temperature_2m_min", i),
                weekday: weekday_of(&date).to_string(),
                date: fmt_date(&date),
            }
        })
        .collect();

    let sunrise = day_str("sunrise", 0);
    let sunset = day_str("sunset", 0);
    let sun_frac = match (
        minutes_of(now_iso),
        minutes_of(&sunrise),
        minutes_of(&sunset),
    ) {
        (Some(now), Some(rise), Some(set)) if set > rise => {
            ((now - rise) / (set - rise)).clamp(0.0, 1.0)
        }
        _ => 0.5,
    };

    Ok(WeatherData {
        temp,
        weather_code,
        fetched_at: now_unix(),
        feels_like,
        humidity,
        wind_speed,
        datetime: format!("{}  {}", fmt_date(now_iso), fmt_time(now_iso)),
        time_short: fmt_time(now_iso),
        forecast,
        rain_chance: at_hour(&rain_24h).round().clamp(0.0, 100.0) as u8,
        uv_index: at_hour(&series("uv_index")),
        visibility_km: at_hour(&series("visibility")) / 1000.0,
        sunrise: fmt_time(&sunrise),
        sunset: fmt_time(&sunset),
        sun_frac,
        hourly,
        wind_24h,
        rain_24h,
    })
}

/// Resolve a query to a list of candidate cities via Open-Meteo's geocoding API.
/// Each candidate's label includes the country code when available
/// (e.g. "Porto, PT"). An empty/missing `results` is `Ok(vec![])` (no matches),
/// not an error.
fn geocode_search(query: &str) -> Result<Vec<Candidate>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[
            ("name", q),
            ("count", "10"),
            ("language", "en"),
            ("format", "json"),
        ])
        .send()
        .map_err(|e| format!("network error: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("server returned {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .map_err(|e| format!("could not parse response: {e}"))?;

    let Some(results) = json["results"].as_array() else {
        return Ok(vec![]);
    };

    let candidates = results
        .iter()
        .filter_map(|r| {
            let lat = r["latitude"].as_f64()?;
            let lon = r["longitude"].as_f64()?;
            let name = r["name"].as_str()?;
            let country = r["country_code"].as_str().or_else(|| r["country"].as_str());
            let label = match country {
                Some(c) if !c.is_empty() => format!("{name}, {c}"),
                _ => name.to_string(),
            };
            Some(Candidate { label, lat, lon })
        })
        .collect();
    Ok(candidates)
}

// ── Cache (a coordinate-keyed map persisted as JSON) ────────────────────────

/// Stable cache key for a coordinate pair.
fn coord_key(lat: f64, lon: f64) -> String {
    format!("{lat:.4},{lon:.4}")
}

/// Read the whole cache map, if the file exists and parses.
fn read_cache_map() -> Option<HashMap<String, WeatherData>> {
    let raw = std::fs::read_to_string(CACHE_PATH).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Insert/replace one city's entry in the cache map (best-effort).
fn write_cache_entry(key: &str, data: &WeatherData) {
    let mut map = read_cache_map().unwrap_or_default();
    map.insert(key.to_string(), data.clone());
    if let Ok(raw) = serde_json::to_string(&map) {
        if let Some(parent) = std::path::Path::new(CACHE_PATH).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(CACHE_PATH, raw);
    }
}

/// Current unix time in seconds (0 if the clock is before the epoch).
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ── Pure helpers (testable) ─────────────────────────────────────────────────

/// Human-readable description for a WMO weather code group.
pub fn weather_description(code: u32) -> &'static str {
    match code {
        0 => "Clear",
        1..=2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51..=67 => "Rain / Drizzle",
        71..=77 => "Snow",
        80..=82 => "Showers",
        95..=99 => "Thunderstorm",
        _ => "Unknown",
    }
}

/// A narrow (single-cell) glyph for a WMO weather code group — used where the
/// double-width emoji of [`weather_symbol`] would break the layout.
pub fn weather_glyph(code: u32) -> &'static str {
    match code {
        0 | 1 => "☀",
        2 => "⛅",
        3 => "☁",
        45 | 48 => "≋",
        51..=67 | 80..=82 => "☂",
        71..=77 | 85 | 86 => "❄",
        95..=99 => "⛈",
        _ => "☁",
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn parse_ymd(s: &str) -> Option<(i64, i64, i64)> {
    Some((
        s.get(0..4)?.parse().ok()?,
        s.get(5..7)?.parse().ok()?,
        s.get(8..10)?.parse().ok()?,
    ))
}

/// Minutes since local midnight from an ISO timestamp ("2026-08-11T15:05").
fn minutes_of(iso: &str) -> Option<f64> {
    let h: f64 = iso.get(11..13)?.parse().ok()?;
    let m: f64 = iso.get(14..16)?.parse().ok()?;
    Some(h * 60.0 + m)
}

/// "2026-08-11" (or a full ISO timestamp) -> "11 August, 2026".
pub fn fmt_date(iso: &str) -> String {
    match parse_ymd(iso) {
        Some((y, m, d)) if (1..=12).contains(&m) => {
            format!("{} {}, {}", d, MONTHS[(m - 1) as usize], y)
        }
        _ => iso.to_string(),
    }
}

/// ISO timestamp -> "5:01 AM". Returns the input unchanged if it has no time.
pub fn fmt_time(iso: &str) -> String {
    let (Some(h), Some(m)) = (iso.get(11..13), iso.get(14..16)) else {
        return iso.to_string();
    };
    let h: u32 = h.parse().unwrap_or(0);
    let (h12, ap) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{h12}:{m} {ap}")
}

/// Weekday name for a "YYYY-MM-DD" date (Howard Hinnant's civil calendar math).
pub fn weekday_of(date: &str) -> &'static str {
    let Some((y, m, d)) = parse_ymd(date) else {
        return "";
    };
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468; // days since 1970-01-01 (a Thursday)
    [
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
    ][days.rem_euclid(7) as usize]
}

/// A single unicode glyph representing a WMO weather code group.
pub fn weather_symbol(code: u32) -> &'static str {
    match code {
        0 => "☀",
        1..=2 => "⛅",
        3 => "☁",
        45 | 48 => "🌫",
        51..=67 => "🌧",
        71..=77 => "❄",
        80..=82 => "🌦",
        95..=99 => "⛈",
        _ => "?",
    }
}

/// 3-row ASCII art for a WMO weather code group, sized to fit a compact panel.
pub fn weather_art(code: u32) -> [&'static str; 3] {
    match code {
        // Clear / sun
        0 => ["   \\ | /    ", " -- (   ) -- ", "   / | \\    "],
        // Partly cloudy
        1..=2 => ["  \\_`-.__   ", " .-(    ).  ", "(___.__)__) "],
        // Overcast
        3 => ["   .--.    ", " .-(    ).  ", "(___.__)__) "],
        // Fog
        45 | 48 => [" _ - _ - _ ", " - _ - _ - ", " _ - _ - _ "],
        // Rain / drizzle
        51..=67 => [" .-(    ).  ", "(___.__)__) ", "  / / / /   "],
        // Snow
        71..=77 => [" .-(    ).  ", "(___.__)__) ", "  *  *  *   "],
        // Showers
        80..=82 => [" .-(    ).  ", "(___.__)__) ", " / / / / /  "],
        // Thunderstorm
        95..=99 => [" .-(    ).  ", "(___.__)__) ", "   /_  /    "],
        // Default / unknown
        _ => ["    ?      ", "   ???     ", "    ?      "],
    }
}

/// Whether cached data is still fresh: less than 15 minutes (900 s) old.
pub fn cache_is_fresh(fetched_at: u64, now: u64) -> bool {
    now.saturating_sub(fetched_at) < 900
}

/// Whether a query is long enough to warrant a live search (>= 3 chars,
/// ignoring surrounding whitespace).
pub fn should_search(query: &str) -> bool {
    query.trim().chars().count() >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_unit_convert_toggle_label() {
        assert_eq!(TempUnit::Celsius.convert(25.0), 25.0);
        assert_eq!(TempUnit::Fahrenheit.convert(25.0), 77.0);
        assert_eq!(TempUnit::Fahrenheit.convert(0.0), 32.0);
        assert_eq!(TempUnit::Celsius.label(), "°C");
        assert_eq!(TempUnit::Fahrenheit.label(), "°F");
        assert_eq!(TempUnit::Celsius.toggle(), TempUnit::Fahrenheit);
        assert_eq!(TempUnit::Fahrenheit.toggle(), TempUnit::Celsius);
    }

    #[test]
    fn descriptions_cover_groups() {
        assert_eq!(weather_description(0), "Clear");
        assert_eq!(weather_description(2), "Partly cloudy");
        assert_eq!(weather_description(3), "Overcast");
        assert_eq!(weather_description(48), "Fog");
        assert_eq!(weather_description(63), "Rain / Drizzle");
        assert_eq!(weather_description(75), "Snow");
        assert_eq!(weather_description(81), "Showers");
        assert_eq!(weather_description(96), "Thunderstorm");
        assert_eq!(weather_description(1234), "Unknown");
    }

    #[test]
    fn date_time_and_weekday_formatting() {
        assert_eq!(fmt_date("2026-08-11"), "11 August, 2026");
        assert_eq!(fmt_date("2026-08-11T15:05"), "11 August, 2026");
        assert_eq!(fmt_time("2026-08-11T15:05"), "3:05 PM");
        assert_eq!(fmt_time("2026-08-11T00:30"), "12:30 AM");
        assert_eq!(fmt_time("2026-08-11T12:00"), "12:00 PM");
        assert_eq!(fmt_time("nope"), "nope");
        assert_eq!(weekday_of("2026-08-11"), "Tuesday");
        assert_eq!(weekday_of("2022-07-24"), "Sunday");
        assert_eq!(weekday_of(""), "");
        assert_eq!(minutes_of("2026-08-11T15:05"), Some(905.0));
        assert_eq!(minutes_of("bad"), None);
    }

    #[test]
    fn symbols_are_non_empty() {
        for code in [0u32, 1, 3, 45, 55, 73, 81, 95, 9999] {
            assert!(!weather_glyph(code).is_empty(), "code {code} empty glyph");
        }
        for code in [0u32, 1, 3, 45, 55, 73, 81, 95, 9999] {
            assert!(!weather_symbol(code).is_empty(), "code {code} empty symbol");
        }
    }

    #[test]
    fn cache_freshness_boundary() {
        assert!(cache_is_fresh(1000, 1000));
        assert!(cache_is_fresh(1000, 1899));
        assert!(!cache_is_fresh(1000, 1900));
        assert!(!cache_is_fresh(1000, 5000));
        assert!(cache_is_fresh(5000, 1000));
    }

    #[test]
    fn city_presets_non_empty() {
        assert!(!CITY_PRESETS.is_empty());
        for (name, lat, lon) in CITY_PRESETS {
            assert!(!name.is_empty());
            assert!(lat.is_finite() && lon.is_finite());
        }
    }

    #[test]
    fn coord_key_is_stable_and_rounded() {
        assert_eq!(coord_key(38.7223, -9.1393), "38.7223,-9.1393");
        // Same rounded key regardless of tiny precision differences.
        assert_eq!(coord_key(51.50741, -0.12782), coord_key(51.50739, -0.12778));
    }

    #[test]
    fn city_management() {
        let mut p = WeatherPanel::new();
        p.set_cities(vec![(1, "A".into(), 1.0, 1.0), (2, "B".into(), 2.0, 2.0)]);
        assert_eq!(p.cities.len(), 2);
        assert_eq!(p.selected, 0);
        p.select_down();
        assert_eq!(p.selected, 1);
        assert_eq!(p.selected_city_id(), Some(2));
        assert!(p.has_city(2.0, 2.0));
        assert!(!p.has_city(9.0, 9.0));
        p.remove_selected();
        assert_eq!(p.cities.len(), 1);
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn should_search_threshold() {
        assert!(!should_search(""));
        assert!(!should_search("ab"));
        assert!(should_search("abc"));
        assert!(should_search("  abcd "));
    }

    #[test]
    fn weather_art_groups_have_three_rows() {
        for code in [0u32, 1, 2, 3, 45, 48, 55, 67, 73, 81, 95, 99, 9999] {
            let art = weather_art(code);
            assert_eq!(art.len(), 3);
            for row in art {
                assert!(!row.is_empty(), "code {code} has empty art row");
            }
        }
    }

    #[test]
    fn search_navigation_clamps_without_network() {
        let mut p = WeatherPanel::new();
        p.search_open();
        assert!(p.search.active);
        assert_eq!(p.search.status, SearchStatus::Idle);
        // Manually inject candidates (no network) to test navigation clamping.
        p.search.candidates = vec![
            Candidate {
                label: "A".into(),
                lat: 1.0,
                lon: 1.0,
            },
            Candidate {
                label: "B".into(),
                lat: 2.0,
                lon: 2.0,
            },
        ];
        p.search.selected = 0;
        p.search_up(); // clamps at 0
        assert_eq!(p.search.selected, 0);
        p.search_down();
        assert_eq!(p.search.selected, 1);
        p.search_down(); // clamps at last
        assert_eq!(p.search.selected, 1);
        assert_eq!(
            p.selected_candidate().map(|c| c.label.clone()),
            Some("B".into())
        );
        p.search_up();
        assert_eq!(p.search.selected, 0);
        p.search_close();
        assert!(!p.search.active);
        assert!(p.search.candidates.is_empty());
    }

    #[test]
    fn short_query_clears_candidates() {
        let mut p = WeatherPanel::new();
        p.search_open();
        // Push two chars: still below threshold, no search, no candidates.
        p.search_push('a');
        p.search_push('b');
        assert_eq!(p.search.status, SearchStatus::Idle);
        assert!(p.search.candidates.is_empty());
    }
}
