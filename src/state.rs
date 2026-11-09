use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

pub const DOMAIN: &str = "pilvineuvos.com";

pub fn address(inbox: &str) -> String {
    format!("{inbox}@{DOMAIN}")
}

/// Accepts `name` or `name@domain` and returns the inbox name, lowercased to
/// match how the email worker stores it.
pub fn normalize_inbox(input: &str) -> Result<String> {
    let name = input.trim();
    let name = name.split_once('@').map_or(name, |(local, _)| local);
    if name.is_empty() {
        bail!("inbox name must not be empty");
    }
    Ok(name.to_lowercase())
}

fn config_path(name: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("pilvineuvos").join(name))
}

fn state_file() -> Option<PathBuf> {
    config_path("current")
}

fn api_key_file() -> Option<PathBuf> {
    config_path("api_key")
}

pub fn load_current() -> Option<String> {
    let s = fs::read_to_string(state_file()?).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

pub fn save_current(inbox: &str) -> Result<()> {
    let path = state_file().context("could not determine config directory")?;
    write_private(&path, &format!("{inbox}\n"))
}

pub fn load_api_key() -> Option<String> {
    let s = fs::read_to_string(api_key_file()?).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

pub fn save_api_key(key: &str) -> Result<PathBuf> {
    let path = api_key_file().context("could not determine config directory")?;
    write_private(&path, &format!("{key}\n"))?;
    Ok(path)
}

/// Removes the saved key; returns whether one existed.
pub fn delete_api_key() -> Result<bool> {
    let Some(path) = api_key_file() else {
        return Ok(false);
    };
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("failed to remove {}", path.display())),
    }
}

/// Writes a file readable only by the current user.
fn write_private(path: &Path, contents: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    }
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        opts.mode(0o600);
        if path.exists() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .with_context(|| format!("failed to set permissions on {}", path.display()))?;
        }
    }
    let mut file = opts
        .open(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.write_all(contents.as_bytes())
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
        Some(c) => normalize_inbox(&c),
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
            format!("{p}-{suffix}").to_lowercase()
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
        assert_eq!(normalize_inbox("Testi@PilviNeuvos.com").unwrap(), "testi");
        assert_eq!(normalize_inbox("ÄÖ-Box").unwrap(), "äö-box");
        assert!(normalize_inbox("@pilvineuvos.com").is_err());
    }

    #[test]
    fn random_names_have_expected_shape() {
        assert_eq!(random_inbox(None).split('-').count(), 3);
        let p = random_inbox(Some("test"));
        assert!(random_inbox(Some("CI")).starts_with("ci-"));
        assert!(p.starts_with("test-") && p.len() == "test-".len() + 6);
    }
}
