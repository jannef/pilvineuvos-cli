use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

pub const DOMAIN: &str = "pilvineuvos.com";

pub fn address(inbox: &str) -> String {
    format!("{inbox}@{DOMAIN}")
}

/// Accepts `name` or `name@domain` and returns the inbox name.
pub fn normalize_inbox(input: &str) -> Result<String> {
    let name = input.trim();
    let name = name.split_once('@').map_or(name, |(local, _)| local);
    if name.is_empty() {
        bail!("inbox name must not be empty");
    }
    Ok(name.to_string())
}

fn state_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("pilvineuvos").join("current"))
}

pub fn load_current() -> Option<String> {
    let s = fs::read_to_string(state_file()?).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

pub fn save_current(inbox: &str) -> Result<()> {
    let path = state_file().context("could not determine config directory")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    }
    fs::write(&path, format!("{inbox}\n"))
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Explicit argument → `PILVINEUVOS_INBOX` → saved current inbox.
pub fn resolve_inbox(arg: Option<&str>) -> Result<String> {
    if let Some(a) = arg {
        return normalize_inbox(a);
    }
    if let Ok(env) = std::env::var("PILVINEUVOS_INBOX")
        && !env.trim().is_empty()
    {
        return normalize_inbox(&env);
    }
    match load_current() {
        Some(c) => Ok(c),
        None => bail!(
            "no inbox given; pass one, set PILVINEUVOS_INBOX, or run `pnmail new` / `pnmail use <inbox>`"
        ),
    }
}

const ADJECTIVES: &[&str] = &[
    "amber", "bold", "brisk", "calm", "clever", "cosmic", "crisp", "dusty", "eager", "fuzzy",
    "gentle", "golden", "happy", "hidden", "icy", "jolly", "lucky", "mellow", "misty", "nimble",
    "proud", "quiet", "rapid", "rusty", "shy", "silent", "snowy", "sunny", "swift", "witty",
];
const NOUNS: &[&str] = &[
    "badger", "birch", "cloud", "comet", "falcon", "fern", "fjord", "fox", "harbor", "heron",
    "island", "lake", "lynx", "maple", "meadow", "moose", "otter", "owl", "pine", "raven", "reef",
    "river", "seal", "spruce", "stone", "storm", "tundra", "wave", "willow", "wolf",
];

fn random_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    h.finish()
}

pub fn random_inbox(prefix: Option<&str>) -> String {
    let mut r = random_u64();
    let mut next = |n: u64| {
        let v = r % n;
        r /= n;
        v as usize
    };
    match prefix {
        Some(p) => {
            const CHARS: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
            let suffix: String = (0..6)
                .map(|_| CHARS[next(CHARS.len() as u64)] as char)
                .collect();
            format!("{p}-{suffix}")
        }
        None => format!(
            "{}-{}-{:04}",
            ADJECTIVES[next(ADJECTIVES.len() as u64)],
            NOUNS[next(NOUNS.len() as u64)],
            next(10_000)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_inbox_names() {
        assert_eq!(normalize_inbox("foo").unwrap(), "foo");
        assert_eq!(normalize_inbox(" foo@pilvineuvos.com ").unwrap(), "foo");
        assert!(normalize_inbox("@pilvineuvos.com").is_err());
    }

    #[test]
    fn random_names_have_expected_shape() {
        assert_eq!(random_inbox(None).split('-').count(), 3);
        let p = random_inbox(Some("test"));
        assert!(p.starts_with("test-") && p.len() == "test-".len() + 6);
    }
}
