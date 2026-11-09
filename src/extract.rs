use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https?://[^\s<>"'`\])}]+"#).unwrap());
static HREF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)href\s*=\s*["']([^"']+)["']"#).unwrap());
static DIGITS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{3}[- ]\d{3}|\d{4,8})\b").unwrap());
static ALNUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[A-Z0-9]{6,8}\b").unwrap());

const KEYWORDS: &[&str] = &[
    "code",
    "koodi",
    "verification",
    "verify",
    "vahvistus",
    "otp",
    "pin",
    "one-time",
    "passcode",
    "password",
    "token",
    "security",
    "login",
    "sign in",
    "tunnusluku",
];

/// All http(s) links in the text and HTML bodies, deduplicated, in order of appearance.
pub fn links(text: &str, html: Option<&str>) -> Vec<String> {
    let mut found = Vec::new();
    if let Some(h) = html {
        found.extend(HREF.captures_iter(h).map(|c| decode_entities(&c[1])));
        found.extend(URL.find_iter(h).map(|m| decode_entities(m.as_str())));
    }
    found.extend(URL.find_iter(text).map(|m| m.as_str().to_string()));

    let mut seen = HashSet::new();
    found
        .into_iter()
        .map(|u| {
            u.trim_end_matches(['.', ',', ';', ':', '!', '?'])
                .to_string()
        })
        .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
        .filter(|u| seen.insert(u.clone()))
        .collect()
}

/// Best guess at a one-time code in the subject or body.
pub fn code(subject: &str, text: &str) -> Option<String> {
    let mut best: Option<(i32, String)> = None;
    for (src, bonus) in [(subject, 4), (text, 0)] {
        let cleaned = URL.replace_all(src, " ");
        let lower = cleaned.to_ascii_lowercase();
        let candidates = DIGITS
            .find_iter(&cleaned)
            .chain(ALNUM.find_iter(&cleaned).filter(|m| {
                let s = m.as_str();
                s.bytes().any(|b| b.is_ascii_digit()) && s.bytes().any(|b| b.is_ascii_alphabetic())
            }));
        for m in candidates {
            let s = score(&lower, m.start(), m.end(), m.as_str()) + bonus;
            if s > 0 && best.as_ref().is_none_or(|(b, _)| s > *b) {
                best = Some((s, m.as_str().to_string()));
            }
        }
    }
    best.map(|(_, c)| c)
}

fn score(lower: &str, start: usize, end: usize, token: &str) -> i32 {
    let digits: String = token.chars().filter(char::is_ascii_digit).collect();
    let mut s = match token.len() {
        6 => 3,
        _ => 1,
    };
    if token.len() == 4
        && digits.len() == 4
        && (digits.starts_with("19") || digits.starts_with("20"))
    {
        s -= 5;
    }
    // Part of a longer number such as a phone number, price, or date.
    let before = lower[..start].chars().next_back();
    let after = lower[end..].chars().next();
    if matches!(before, Some('+' | '$' | '€' | '/' | '.' | ':' | '#'))
        || matches!(after, Some('/' | '€' | '%' | ':'))
        || (matches!(after, Some('.' | ','))
            && lower[end..]
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_digit()))
    {
        s -= 4;
    }

    let window_start = floor_char_boundary(lower, start.saturating_sub(80));
    let preceding = &lower[window_start..start];
    if let Some(dist) = KEYWORDS
        .iter()
        .filter_map(|k| preceding.rfind(k).map(|i| preceding.len() - (i + k.len())))
        .min()
    {
        s += 10 - (dist / 10) as i32;
    }
    let window_end = floor_char_boundary(lower, (end + 40).min(lower.len()));
    if KEYWORDS.iter().any(|k| lower[end..window_end].contains(k)) {
        s += 4;
    }
    s
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#38;", "&")
        .replace("&quot;", "\"")
        .replace("&#x3D;", "=")
        .replace("&#61;", "=")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_code_near_keyword() {
        assert_eq!(
            code("Welcome", "Hi!\nYour verification code is 482913.\nThanks").as_deref(),
            Some("482913")
        );
        assert_eq!(code("", "Use 7731 as your PIN").as_deref(), Some("7731"));
        assert_eq!(
            code("", "Your login code: K7XQ2P").as_deref(),
            Some("K7XQ2P")
        );
    }

    #[test]
    fn prefers_code_over_noise() {
        let body = "Order 2026 from 12/10/2026, call +358401234567.\n\nYour one-time code is 551 204.\n© 2026 Example Ltd";
        assert_eq!(code("", body).as_deref(), Some("551 204"));
    }

    #[test]
    fn subject_code_wins() {
        assert_eq!(
            code("123456 is your Example code", "Ref 9988776").as_deref(),
            Some("123456")
        );
    }

    #[test]
    fn ignores_years_and_urls() {
        assert_eq!(
            code("", "Copyright 2026. Visit https://example.com/a/837261"),
            None
        );
    }

    #[test]
    fn extracts_links() {
        let html = r#"<a href="https://example.com/verify?t=1&amp;u=2">Verify</a> <img src="https://cdn.example.com/x.png">"#;
        let text = "Open https://example.com/verify?t=1&u=2. Or https://example.com/help, thanks";
        assert_eq!(
            links(text, Some(html)),
            vec![
                "https://example.com/verify?t=1&u=2",
                "https://cdn.example.com/x.png",
                "https://example.com/help",
            ]
        );
    }
}
