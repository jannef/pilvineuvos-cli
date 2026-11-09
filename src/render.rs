use std::io::IsTerminal;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::Summary;
use crate::message::Parsed;

/// Index entries are purged after 23h; raw messages after 24h.
pub const VISIBLE_FOR_MS: i64 = 23 * 60 * 60 * 1000;

fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal())
}

fn paint(s: &str, code: &str) -> String {
    if color_enabled() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    paint(s, "1")
}
pub fn dim(s: &str) -> String {
    paint(s, "2")
}
pub fn red(s: &str) -> String {
    paint(s, "31")
}
pub fn green(s: &str) -> String {
    paint(s, "32")
}
pub fn cyan(s: &str) -> String {
    paint(s, "36")
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Compact duration: `45s`, `12m`, `3h 05m`.
pub fn human_ms(ms: i64) -> String {
    let secs = ms.max(0) / 1000;
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        _ => format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60),
    }
}

pub fn age(received_at: i64) -> String {
    format!("{} ago", human_ms(now_ms() - received_at))
}

/// Conservative remaining lifetime, and whether it is under an hour.
fn expiry_plain(received_at: i64) -> (String, bool) {
    let left = received_at + VISIBLE_FOR_MS - now_ms();
    let text = if left <= 0 {
        "expiring".to_string()
    } else {
        format!("~{}", human_ms(left))
    };
    (text, left < 60 * 60 * 1000)
}

pub fn expiry(received_at: i64) -> String {
    let (text, urgent) = expiry_plain(received_at);
    if urgent { red(&text) } else { text }
}

pub fn truncate(s: &str, max: usize) -> String {
    let s: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if s.chars().count() <= max {
        s
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

fn pad(s: &str, width: usize) -> String {
    format!("{s:<width$}")
}

pub fn summary_table(rows: &[Summary]) {
    let width: usize = std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(120);
    let id_w = rows
        .iter()
        .map(|r| r.id.chars().count())
        .max()
        .unwrap_or(2)
        .max(2);
    let from_w = 28;
    let fixed = id_w + 12 + 10 + from_w + 4 * 2;
    let subj_w = width.saturating_sub(fixed).max(20);

    println!(
        "{}",
        bold(&format!(
            "{}  {}  {}  {}  SUBJECT",
            pad("ID", id_w),
            pad("AGE", 12),
            pad("EXPIRES", 10),
            pad("FROM", from_w)
        ))
    );
    for r in rows {
        let (exp, urgent) = expiry_plain(r.received_at);
        let exp = if urgent {
            red(&pad(&exp, 10))
        } else {
            pad(&exp, 10)
        };
        println!(
            "{}  {}  {}  {}  {}",
            cyan(&pad(&r.id, id_w)),
            dim(&pad(&age(r.received_at), 12)),
            exp,
            pad(
                &truncate(r.sender.as_deref().unwrap_or("-"), from_w),
                from_w
            ),
            truncate(r.subject.as_deref().unwrap_or("(no subject)"), subj_w)
        );
    }
}

/// One line per message, used by `watch`.
pub fn summary_line(r: &Summary) {
    println!(
        "{}  {}  {}  {}",
        dim(&age(r.received_at)),
        cyan(&r.id),
        r.sender.as_deref().unwrap_or("-"),
        bold(r.subject.as_deref().unwrap_or("(no subject)"))
    );
}

pub struct ViewOpts {
    pub html: bool,
    pub headers: bool,
}

pub fn message(p: &Parsed, received_at: Option<i64>, opts: &ViewOpts) {
    let label = |l: &str| dim(&format!("{l:<9}"));
    if opts.headers {
        println!("{}", dim(&p.raw_headers));
    } else {
        println!("{}{}", label("From:"), p.from.as_deref().unwrap_or("-"));
        if !p.to.is_empty() {
            println!("{}{}", label("To:"), p.to.join(", "));
        }
        println!(
            "{}{}",
            label("Subject:"),
            bold(p.subject.as_deref().unwrap_or("(no subject)"))
        );
        if let Some(d) = &p.date {
            let rel = p
                .timestamp
                .map(|t| format!(" ({})", age(t * 1000)))
                .unwrap_or_default();
            println!("{}{d}{}", label("Date:"), dim(&rel));
        }
    }
    if let Some(r) = received_at {
        println!("{}{}", label("Expires:"), expiry(r));
    }
    println!("{}{}", label("ID:"), cyan(&p.id));
    for a in &p.attachments {
        println!(
            "{}{} {}",
            label("Attach:"),
            a.name,
            dim(&format!("({}, {})", a.content_type, human_bytes(a.size)))
        );
    }
    println!("{}", dim(&"─".repeat(60)));
    let body = if opts.html {
        p.html.as_deref().unwrap_or("(message has no HTML part)")
    } else {
        p.text.as_str()
    };
    println!("{}", body.trim_end());
}

pub fn human_bytes(n: usize) -> String {
    match n {
        0..1024 => format!("{n} B"),
        1024..1_048_576 => format!("{:.1} KiB", n as f64 / 1024.0),
        _ => format!("{:.1} MiB", n as f64 / 1_048_576.0),
    }
}

pub fn success(s: &str) -> String {
    green(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations() {
        assert_eq!(human_ms(45_000), "45s");
        assert_eq!(human_ms(12 * 60_000), "12m");
        assert_eq!(human_ms(3 * 3_600_000 + 5 * 60_000), "3h 05m");
        assert_eq!(human_ms(-5), "0s");
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 6), "hello…");
    }
}
