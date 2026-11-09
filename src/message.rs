use anyhow::{Result, anyhow};
use mail_parser::{AddressList, MessageParser, PartKind};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Attachment {
    pub name: String,
    pub content_type: String,
    pub size: usize,
    #[serde(skip)]
    pub data: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct Parsed {
    pub id: String,
    pub from: Option<String>,
    pub to: Vec<String>,
    pub subject: Option<String>,
    /// RFC 3339 `Date:` header.
    pub date: Option<String>,
    /// `Date:` header as epoch seconds.
    #[serde(skip)]
    pub timestamp: Option<i64>,
    /// Plain-text body; derived from HTML when the message has no text part.
    pub text: String,
    pub html: Option<String>,
    pub attachments: Vec<Attachment>,
    #[serde(skip)]
    pub raw_headers: String,
}

pub fn parse(id: &str, raw: &[u8]) -> Result<Parsed> {
    let msg = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| anyhow!("message {id} could not be parsed as e-mail"))?;

    let html: Vec<String> = msg
        .html_body()
        .filter(|p| matches!(p.kind(), PartKind::Html))
        .filter_map(|p| p.text().map(|t| t.into_owned()))
        .collect();
    let html = (!html.is_empty()).then(|| html.join("\n"));

    let plain: Vec<String> = msg
        .text_body()
        .filter(|p| matches!(p.kind(), PartKind::Text))
        .filter_map(|p| p.text().map(|t| t.into_owned()))
        .collect();
    let text = if !plain.is_empty() {
        plain.join("\n")
    } else if let Some(h) = &html {
        html_to_text(h)
    } else {
        String::new()
    };

    let attachments = msg
        .attachments()
        .map(|p| {
            let content_type = p
                .content_type()
                .map(|ct| match ct.subtype() {
                    Some(sub) => format!("{}/{}", ct.ctype(), sub),
                    None => ct.ctype().to_string(),
                })
                .unwrap_or_else(|| "application/octet-stream".into());
            let data = p.decoded().into_owned();
            Attachment {
                name: p.attachment_name().unwrap_or("attachment").to_string(),
                content_type,
                size: data.len(),
                data,
            }
        })
        .collect();

    Ok(Parsed {
        id: id.to_string(),
        from: msg.from().as_ref().and_then(first_address),
        to: msg.to().map(|l| all_addresses(&l)).unwrap_or_default(),
        subject: msg.subject().map(str::to_string),
        date: msg.date().map(|d| d.to_rfc3339()),
        timestamp: msg.date().map(|d| d.to_timestamp()),
        text,
        html,
        attachments,
        raw_headers: header_block(raw),
    })
}

pub fn html_to_text(html: &str) -> String {
    html2text::from_read(html.as_bytes(), 100).unwrap_or_else(|_| html.to_string())
}

fn format_mailbox(name: Option<&str>, addr: Option<&str>) -> Option<String> {
    match (name, addr) {
        (Some(n), Some(a)) if !n.is_empty() => Some(format!("{n} <{a}>")),
        (_, Some(a)) => Some(a.to_string()),
        (Some(n), None) => Some(n.to_string()),
        (None, None) => None,
    }
}

fn first_address(list: &AddressList) -> Option<String> {
    list.first()
        .and_then(|m| format_mailbox(m.name(), m.address()))
}

fn all_addresses(list: &AddressList) -> Vec<String> {
    list.mailboxes()
        .filter_map(|m| format_mailbox(m.name(), m.address()))
        .collect()
}

fn header_block(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let end = [text.find("\r\n\r\n"), text.find("\n\n")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(text.len());
    text[..end].replace("\r\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const MULTIPART: &str = "From: Example Service <noreply@example.com>\r\n\
To: brisk-otter-1234@pilvineuvos.com\r\n\
Subject: Your verification code\r\n\
Date: Mon, 09 Nov 2026 12:00:00 +0000\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"outer\"\r\n\
\r\n\
--outer\r\n\
Content-Type: multipart/alternative; boundary=\"alt\"\r\n\
\r\n\
--alt\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Your code is 482913.\r\n\
--alt\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<p>Your code is <b>482913</b>. <a href=\"https://example.com/verify?t=1&amp;u=2\">Verify</a></p>\r\n\
--alt--\r\n\
--outer\r\n\
Content-Type: text/csv; name=\"data.csv\"\r\n\
Content-Disposition: attachment; filename=\"data.csv\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
YSxiCjEsMgo=\r\n\
--outer--\r\n";

    #[test]
    fn parses_multipart_message() {
        let p = parse("m1", MULTIPART.as_bytes()).unwrap();
        assert_eq!(
            p.from.as_deref(),
            Some("Example Service <noreply@example.com>")
        );
        assert_eq!(p.to, vec!["brisk-otter-1234@pilvineuvos.com"]);
        assert_eq!(p.subject.as_deref(), Some("Your verification code"));
        assert!(p.text.contains("482913"));
        assert!(p.html.as_deref().unwrap().contains("<b>482913</b>"));
        assert_eq!(p.attachments.len(), 1);
        assert_eq!(p.attachments[0].name, "data.csv");
        assert_eq!(p.attachments[0].data, b"a,b\n1,2\n");
        assert!(
            p.raw_headers.starts_with("From:") && p.raw_headers.ends_with("boundary=\"outer\"")
        );
    }

    #[test]
    fn html_only_message_gets_text_body() {
        let raw = "From: a@b.c\r\nSubject: x\r\nContent-Type: text/html\r\n\r\n<p>Hello <b>world</b></p>\r\n";
        let p = parse("m2", raw.as_bytes()).unwrap();
        assert!(p.text.contains("Hello") && p.text.contains("world"));
        assert!(!p.text.contains("<p>"));
    }
}
