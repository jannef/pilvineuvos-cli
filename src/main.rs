mod api;
mod extract;
mod message;
mod render;
mod state;

use std::collections::HashSet;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};
use regex::Regex;
use serde_json::json;

use api::{Client, Summary};
use message::Parsed;

#[derive(Parser)]
#[command(
    name = "pnmail",
    version,
    about = "Throwaway inboxes at <name>@pilvineuvos.com",
    after_help = "Messages are kept for ~23-24 hours.\n\nExamples:\n  pnmail new              create a random inbox and make it current\n  pnmail wait --code      block until mail arrives, print the one-time code\n  pnmail ls               list messages in the current inbox\n  pnmail read             show the newest message"
)]
struct Cli {
    /// API base URL
    #[arg(long, global = true, env = "PILVINEUVOS_API", default_value = api::DEFAULT_API)]
    api: String,

    /// API key [default: the key saved by `pnmail login`]
    #[arg(
        long,
        global = true,
        env = "PILVINEUVOS_API_KEY",
        hide_env_values = true
    )]
    api_key: Option<String>,

    /// Machine-readable JSON output
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Save an API key (prompts for it if not given, which keeps it out of shell history)
    Login { key: Option<String> },
    /// Remove the saved API key
    Logout,
    /// Create a random inbox and make it the current one
    New {
        /// Use `<PREFIX>-xxxxxx` instead of a random word pair
        #[arg(long)]
        prefix: Option<String>,
        /// Don't make it the current inbox
        #[arg(long)]
        no_save: bool,
    },
    /// Set the current inbox
    Use { inbox: String },
    /// Show the current inbox and address
    Whoami,
    /// List messages in an inbox
    #[command(visible_alias = "list")]
    Ls { inbox: Option<String> },
    /// Show a message (newest in the inbox if no ID is given)
    Read {
        #[command(flatten)]
        target: Target,
        /// Print the HTML body instead of text
        #[arg(long)]
        html: bool,
        /// Show all headers
        #[arg(long)]
        headers: bool,
        /// Dump the raw .eml to stdout
        #[arg(long, conflicts_with_all = ["html", "headers"])]
        raw: bool,
    },
    /// Save a message as .eml (newest if no ID is given)
    Save {
        #[command(flatten)]
        target: Target,
        /// Output file [default: <id>.eml]
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Also extract attachments into this directory
        #[arg(long, value_name = "DIR")]
        attachments: Option<PathBuf>,
    },
    /// Wait for a new message to arrive (exit code 2 on timeout)
    Wait {
        inbox: Option<String>,
        #[command(flatten)]
        poll: Poll,
        /// Give up after this long (e.g. 90, 30s, 5m)
        #[arg(long, default_value = "2m", value_parser = parse_duration)]
        timeout: Duration,
        /// Also accept messages received this long before waiting started
        #[arg(long, value_parser = parse_duration)]
        since: Option<Duration>,
        /// Only match senders matching this regex (case-insensitive)
        #[arg(long, value_parser = parse_regex)]
        from: Option<Regex>,
        /// Only match subjects matching this regex (case-insensitive)
        #[arg(long, value_parser = parse_regex)]
        subject: Option<Regex>,
        /// Print only the one-time code
        #[arg(long, conflicts_with = "link")]
        code: bool,
        /// Print only the first link
        #[arg(long)]
        link: bool,
    },
    /// Print new messages as they arrive, until interrupted
    Watch {
        inbox: Option<String>,
        #[command(flatten)]
        poll: Poll,
    },
    /// Extract a one-time code from a message
    Code {
        #[command(flatten)]
        target: Target,
    },
    /// Extract links from a message
    Links {
        #[command(flatten)]
        target: Target,
    },
}

#[derive(Args)]
struct Target {
    /// Message ID [default: newest message in the inbox]
    #[arg(add = ArgValueCandidates::new(message_id_candidates))]
    id: Option<String>,
    /// Inbox to take the newest message from
    #[arg(short, long)]
    inbox: Option<String>,
}

#[derive(Args)]
struct Poll {
    /// Polling interval
    #[arg(long, default_value = "3s", value_parser = parse_duration)]
    interval: Duration,
}

fn main() -> ExitCode {
    // Answers shell completion requests (COMPLETE=<shell>) and exits; otherwise a no-op.
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{} {e:#}", render::red("error:"));
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let json = cli.json;
    let api_key = cli.api_key.clone().or_else(state::load_api_key);
    let client = || Client::new(&cli.api, api_key.clone());

    match cli.cmd {
        Cmd::Login { key } => {
            let key = match key {
                Some(k) => k,
                None => read_key()?,
            };
            let key = key.trim().to_lowercase();
            if key.is_empty() {
                bail!("API key must not be empty");
            }
            // Any inbox works for checking the key; an empty list means it was accepted.
            match Client::new(&cli.api, Some(key.clone()))?.list("pnmail-key-check") {
                Err(e) if e.is::<api::Unauthorized>() => bail!("the API rejected this key"),
                result => result?,
            };
            let path = state::save_api_key(&key)?;
            if json {
                print_json(&json!({ "saved": path }))?;
            } else {
                println!(
                    "{} API key saved to {}",
                    render::success("ok"),
                    path.display()
                );
            }
        }
        Cmd::Logout => {
            let removed = state::delete_api_key()?;
            if json {
                print_json(&json!({ "removed": removed }))?;
            } else if removed {
                println!("API key removed");
            } else {
                println!("No saved API key");
            }
        }
        Cmd::New { prefix, no_save } => {
            let inbox = state::random_inbox(prefix.as_deref());
            if !no_save {
                state::save_current(&inbox)?;
            }
            print_inbox(&inbox, json);
        }
        Cmd::Use { inbox } => {
            let inbox = state::normalize_inbox(&inbox)?;
            state::save_current(&inbox)?;
            print_inbox(&inbox, json);
        }
        Cmd::Whoami => {
            let inbox = state::resolve_inbox(None)?;
            print_inbox(&inbox, json);
        }
        Cmd::Ls { inbox } => {
            let inbox = state::resolve_inbox(inbox.as_deref())?;
            let rows = client()?.list(&inbox)?;
            if json {
                print_json(&rows)?;
            } else if rows.is_empty() {
                println!(
                    "No messages for {} in the last 24h.",
                    state::address(&inbox)
                );
            } else {
                render::summary_table(&rows);
            }
        }
        Cmd::Read {
            target,
            html,
            headers,
            raw,
        } => {
            let client = client()?;
            let (id, received_at) = resolve_target(&client, &target)?;
            let bytes = client.fetch_raw(&id)?;
            if raw {
                std::io::stdout().write_all(&bytes)?;
            } else {
                let p = message::parse(&id, &bytes)?;
                if json {
                    print_json(&p)?;
                } else {
                    render::message(&p, received_at, &render::ViewOpts { html, headers });
                }
            }
        }
        Cmd::Save {
            target,
            output,
            attachments,
        } => {
            let client = client()?;
            let (id, _) = resolve_target(&client, &target)?;
            let bytes = client.fetch_raw(&id)?;
            let path = output.unwrap_or_else(|| PathBuf::from(format!("{}.eml", sanitize(&id))));
            fs::write(&path, &bytes)
                .with_context(|| format!("failed to write {}", path.display()))?;
            let mut saved = vec![path];
            if let Some(dir) = attachments {
                saved.extend(save_attachments(&message::parse(&id, &bytes)?, &dir)?);
            }
            if json {
                print_json(&json!({ "id": id, "files": saved }))?;
            } else {
                for p in saved {
                    println!("{} {}", render::success("saved"), p.display());
                }
            }
        }
        Cmd::Wait {
            inbox,
            poll,
            timeout,
            since,
            from,
            subject,
            code,
            link,
        } => {
            let inbox = state::resolve_inbox(inbox.as_deref())?;
            let client = client()?;
            let filter = |m: &Summary| {
                from.as_ref()
                    .is_none_or(|re| re.is_match(m.sender.as_deref().unwrap_or("")))
                    && subject
                        .as_ref()
                        .is_none_or(|re| re.is_match(m.subject.as_deref().unwrap_or("")))
            };
            if std::io::stderr().is_terminal() {
                eprintln!(
                    "{}",
                    render::dim(&format!(
                        "Waiting for mail to {} (timeout {})…",
                        state::address(&inbox),
                        render::human_ms(timeout.as_millis() as i64)
                    ))
                );
            }
            let Some(found) = wait_for(&client, &inbox, poll.interval, timeout, since, filter)?
            else {
                eprintln!(
                    "{} no matching message within the timeout",
                    render::red("timeout:")
                );
                return Ok(ExitCode::from(2));
            };
            let p = message::parse(&found.id, &client.fetch_raw(&found.id)?)?;
            if code {
                print_code(&p, json)?;
            } else if link {
                print_link(&p, json)?;
            } else if json {
                print_json(&p)?;
            } else {
                render::message(
                    &p,
                    Some(found.received_at),
                    &render::ViewOpts {
                        html: false,
                        headers: false,
                    },
                );
            }
        }
        Cmd::Watch { inbox, poll } => {
            let inbox = state::resolve_inbox(inbox.as_deref())?;
            let client = client()?;
            let mut seen: HashSet<String> =
                client.list(&inbox)?.into_iter().map(|m| m.id).collect();
            if std::io::stderr().is_terminal() {
                eprintln!(
                    "{}",
                    render::dim(&format!(
                        "Watching {} (Ctrl-C to stop)…",
                        state::address(&inbox)
                    ))
                );
            }
            loop {
                sleep(poll.interval);
                let rows = match client.list(&inbox) {
                    Ok(rows) => rows,
                    Err(e) if e.is::<api::Unauthorized>() => return Err(e),
                    Err(e) => {
                        eprintln!("{} {e:#}", render::red("warning:"));
                        continue;
                    }
                };
                for m in rows.iter().rev().filter(|m| !seen.contains(&m.id)) {
                    if json {
                        println!("{}", serde_json::to_string(m)?);
                    } else {
                        render::summary_line(m);
                    }
                    std::io::stdout().flush()?;
                }
                seen.extend(rows.into_iter().map(|m| m.id));
            }
        }
        Cmd::Code { target } => {
            let client = client()?;
            let (id, _) = resolve_target(&client, &target)?;
            print_code(&message::parse(&id, &client.fetch_raw(&id)?)?, json)?;
        }
        Cmd::Links { target } => {
            let client = client()?;
            let (id, _) = resolve_target(&client, &target)?;
            let p = message::parse(&id, &client.fetch_raw(&id)?)?;
            let links = extract::links(&p.text, p.html.as_deref());
            if json {
                print_json(&json!({ "id": p.id, "links": links }))?;
            } else if links.is_empty() {
                bail!("no links found in message {}", p.id);
            } else {
                for l in links {
                    println!("{l}");
                }
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Message IDs in the current inbox for shell completion, newest first, with
/// subject and age as the description. Returns nothing on any error so a
/// missing key or network problem never breaks the shell.
fn message_id_candidates() -> Vec<CompletionCandidate> {
    let Ok(inbox) = state::resolve_inbox(None) else {
        return Vec::new();
    };
    let base = std::env::var("PILVINEUVOS_API").unwrap_or_else(|_| api::DEFAULT_API.to_string());
    let key = std::env::var("PILVINEUVOS_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .or_else(state::load_api_key);
    let Ok(client) = Client::with_timeout(&base, key, Duration::from_secs(1)) else {
        return Vec::new();
    };
    client
        .list(&inbox)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(i, m)| {
            let subject = render::truncate(m.subject.as_deref().unwrap_or("(no subject)"), 50);
            CompletionCandidate::new(m.id)
                .help(Some(
                    format!("{subject}, {}", render::age(m.received_at)).into(),
                ))
                .display_order(Some(i))
        })
        .collect()
}

/// Explicit ID, or the newest message in the resolved inbox.
fn resolve_target(client: &Client, t: &Target) -> Result<(String, Option<i64>)> {
    if let Some(id) = &t.id {
        return Ok((id.clone(), None));
    }
    let inbox = state::resolve_inbox(t.inbox.as_deref())?;
    let newest = client
        .list(&inbox)?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no messages for {} in the last 24h", state::address(&inbox)))?;
    Ok((newest.id, Some(newest.received_at)))
}

fn wait_for(
    client: &Client,
    inbox: &str,
    interval: Duration,
    timeout: Duration,
    since: Option<Duration>,
    filter: impl Fn(&Summary) -> bool,
) -> Result<Option<Summary>> {
    let deadline = Instant::now() + timeout;
    let cutoff = since.map(|s| render::now_ms() - s.as_millis() as i64);
    let mut seen: HashSet<String> = client
        .list(inbox)?
        .into_iter()
        .filter(|m| cutoff.is_none_or(|c| m.received_at < c))
        .map(|m| m.id)
        .collect();
    let mut first = true;
    loop {
        if !first {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            sleep(interval.min(left));
        }
        first = false;
        let rows = match client.list(inbox) {
            Ok(rows) => rows,
            Err(e) if e.is::<api::Unauthorized>() => return Err(e),
            Err(e) => {
                eprintln!("{} {e:#}", render::red("warning:"));
                continue;
            }
        };
        // Oldest first, so the first message to arrive wins.
        if let Some(m) = rows
            .iter()
            .rev()
            .find(|m| !seen.contains(&m.id) && filter(m))
        {
            return Ok(Some(m.clone()));
        }
        seen.extend(rows.into_iter().map(|m| m.id));
    }
}

fn print_code(p: &Parsed, json: bool) -> Result<()> {
    let code = extract::code(p.subject.as_deref().unwrap_or(""), &p.text);
    if json {
        return print_json(&json!({ "id": p.id, "code": code }));
    }
    match code {
        Some(c) => {
            println!("{c}");
            Ok(())
        }
        None => bail!("no code found in message {}", p.id),
    }
}

fn print_link(p: &Parsed, json: bool) -> Result<()> {
    let link = extract::links(&p.text, p.html.as_deref())
        .into_iter()
        .next();
    if json {
        return print_json(&json!({ "id": p.id, "link": link }));
    }
    match link {
        Some(l) => {
            println!("{l}");
            Ok(())
        }
        None => bail!("no links found in message {}", p.id),
    }
}

fn read_key() -> Result<String> {
    if std::io::stdin().is_terminal() {
        eprint!("API key: ");
        std::io::stderr().flush()?;
    }
    let mut key = String::new();
    std::io::stdin()
        .read_line(&mut key)
        .context("failed to read API key from stdin")?;
    Ok(key)
}

fn print_inbox(inbox: &str, json: bool) {
    if json {
        println!(
            "{}",
            json!({ "inbox": inbox, "address": state::address(inbox) })
        );
    } else {
        println!("{}", render::bold(&state::address(inbox)));
    }
}

fn print_json<T: serde::Serialize>(v: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

fn save_attachments(p: &Parsed, dir: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let mut used = HashSet::new();
    let mut out = Vec::new();
    for a in &p.attachments {
        let base = sanitize(&a.name);
        let mut name = base.clone();
        let mut n = 1;
        while !used.insert(name.clone()) {
            name = format!("{n}-{base}");
            n += 1;
        }
        let path = dir.join(name);
        fs::write(&path, &a.data).with_context(|| format!("failed to write {}", path.display()))?;
        out.push(path);
    }
    Ok(out)
}

/// Safe single path component.
fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_start_matches('.').trim();
    if s.is_empty() {
        "attachment".into()
    } else {
        s.to_string()
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    let (num, mult) = match s.char_indices().last() {
        Some((i, 's')) => (&s[..i], 1),
        Some((i, 'm')) => (&s[..i], 60),
        Some((i, 'h')) => (&s[..i], 3600),
        _ => (s, 1),
    };
    let n: f64 = num
        .trim()
        .parse()
        .map_err(|_| format!("invalid duration `{s}` (try 90, 30s, 5m, 1h)"))?;
    if !n.is_finite() || n < 0.0 {
        return Err(format!("invalid duration `{s}`"));
    }
    Ok(Duration::from_secs_f64(n * mult as f64))
}

fn parse_regex(s: &str) -> Result<Regex, String> {
    regex::RegexBuilder::new(s)
        .case_insensitive(true)
        .build()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("90").unwrap(), Duration::from_secs(90));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1.5h").unwrap(), Duration::from_secs(5400));
        assert!(parse_duration("soon").is_err());
        assert!(parse_duration("-1s").is_err());
    }

    #[test]
    fn sanitizes_file_names() {
        assert_eq!(sanitize("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize("report 2026.pdf"), "report 2026.pdf");
        assert_eq!(sanitize(".."), "attachment");
    }

    #[test]
    fn cli_is_valid() {
        Cli::command().debug_assert();
    }
}
