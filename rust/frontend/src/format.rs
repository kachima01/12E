//! Display formatting (currency, dates, names, phone numbers).

use wasm_bindgen::JsValue;

pub const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

pub fn now_secs() -> f64 {
    now_ms() / 1000.0
}

fn group_thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// `K1,250` (whole kwacha).
pub fn kwacha(amount: f64) -> String {
    let sign = if amount < 0.0 { "-" } else { "" };
    format!("{sign}K{}", group_thousands(amount.abs().round() as u64))
}

/// `K1,250.50`.
pub fn kwacha_cents(amount: f64) -> String {
    let cents = (amount.abs() * 100.0).round() as u64;
    let sign = if amount < 0.0 { "-" } else { "" };
    format!("{sign}K{}.{:02}", group_thousands(cents / 100), cents % 100)
}

pub fn parse_date(iso: &str) -> Option<js_sys::Date> {
    if iso.is_empty() {
        return None;
    }
    let date = js_sys::Date::new(&JsValue::from_str(iso));
    (!date.get_time().is_nan()).then_some(date)
}

/// `2 Nov 2026`.
pub fn format_date(iso: &str) -> String {
    parse_date(iso)
        .map(|d| format!("{} {} {}", d.get_date(), MONTHS[d.get_month() as usize % 12], d.get_full_year()))
        .unwrap_or_default()
}

/// `2 Nov 2026, 14:05`.
pub fn format_date_time(iso: &str) -> String {
    parse_date(iso)
        .map(|d| {
            format!(
                "{} {} {}, {:02}:{:02}",
                d.get_date(),
                MONTHS[d.get_month() as usize % 12],
                d.get_full_year(),
                d.get_hours(),
                d.get_minutes()
            )
        })
        .unwrap_or_default()
}

pub fn initials(name: &str) -> String {
    let letters: String = name
        .split_whitespace()
        .take(2)
        .filter_map(|part| part.chars().next())
        .flat_map(char::to_uppercase)
        .collect();
    if letters.is_empty() { "12".to_string() } else { letters }
}

pub fn first_name(name: &str) -> String {
    name.split_whitespace().next().unwrap_or("Classmate").to_string()
}

/// Keep the 9 local digits a member types after `+260`.
pub fn phone_input(raw: &str) -> String {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    let local = digits.strip_prefix("260").unwrap_or(&digits);
    let local = local.strip_prefix('0').unwrap_or(local);
    local.chars().take(9).collect()
}

/// `+260971234567` when the number is a valid Zambian mobile number.
pub fn normalize_phone(raw: &str) -> Option<String> {
    let local = phone_input(raw);
    let valid = local.len() == 9 && matches!(local.as_bytes()[0], b'7' | b'9');
    valid.then(|| format!("+260{local}"))
}

pub fn enc(value: &str) -> String {
    String::from(js_sys::encode_uri_component(value))
}
