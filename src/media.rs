//! Files an agent may send, and files strangers send us. A permission question, not a
//! feature — so the rule lives here once instead of in each messaging clapp.
//!
//! **Sending is confined.** Grants are per-command: an agent can hold
//! `Bash(telegram send:*)` and nothing else. If that verb took an absolute path the grant
//! would quietly become "read any file here and publish it" —
//! `telegram send <chat> --file ~/.ssh/id_rsa`. So [`outgoing`] resolves inside
//! `<data>/outbox/` and refuses traversal, absolute paths, and symlinks out of it.
//!
//! That is not a claim a shell-capable agent cannot copy a secret into the outbox first.
//! It is the narrower, honest one: **this app never widens a grant it was given.**
//!
//! **Receiving is quarantined.** Inbound bytes are a stranger's choice: [`store`] renames
//! (never their filename), never marks executable, caps at [`MAX_BYTES`], and hands the
//! agent the path plus the fact that it is untrusted.

use anyhow::{bail, Context, Result};
use std::path::{Component, Path, PathBuf};

/// The most we will write for one inbound file. Telegram's own bot download ceiling is
/// 20 MB and WhatsApp's practical one is similar, so this is generous rather than
/// restrictive — it exists so a hostile or broken peer cannot fill the disk.
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// What kind of thing a file is, decided by extension. Messaging APIs need this to pick
/// a method (`sendPhoto` vs `sendDocument`) and the window needs it to pick a preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
    Audio,
    Document,
}

impl Kind {
    pub fn of(path: &Path) -> Kind {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" | "heif" => Kind::Image,
            "mp4" | "mov" | "webm" | "mkv" | "avi" | "m4v" => Kind::Video,
            "mp3" | "ogg" | "oga" | "opus" | "wav" | "m4a" | "aac" | "flac" => Kind::Audio,
            _ => Kind::Document,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::Audio => "audio",
            Kind::Document => "document",
        }
    }

    /// A reasonable content type for an upload. Deliberately coarse: the receiving service
    /// sniffs anyway, and guessing precisely buys nothing.
    pub fn mime(self, path: &Path) -> &'static str {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "mp4" | "m4v" => "video/mp4",
            "mov" => "video/quicktime",
            "webm" => "video/webm",
            "mp3" => "audio/mpeg",
            "ogg" | "oga" | "opus" => "audio/ogg",
            "wav" => "audio/wav",
            "m4a" | "aac" => "audio/mp4",
            "pdf" => "application/pdf",
            _ => match self {
                Kind::Image => "image/*",
                Kind::Video => "video/*",
                Kind::Audio => "audio/*",
                Kind::Document => "application/octet-stream",
            },
        }
    }
}

/// `<data>/outbox` — the only place an outgoing file may come from. Created on demand so
/// the human can just drop something in it.
pub fn outbox(cli: &str) -> PathBuf {
    let dir = crate::data_subdir(cli, "outbox");
    let _ = crate::ensure_private_dir(&dir);
    dir
}

/// `<data>/inbox` — where received files land, quarantined.
pub fn inbox(cli: &str) -> PathBuf {
    let dir = crate::data_subdir(cli, "inbox");
    let _ = crate::ensure_private_dir(&dir);
    dir
}

/// Resolve a caller-supplied name to a real file **inside the outbox**, or refuse.
///
/// Refused, each for its own reason: an absolute path (the caller is not choosing the
/// root), any `..` (traversal), and a symlink whose target escapes (the classic way a
/// confined directory stops confining). The check is on the CANONICAL path, so it cannot
/// be fooled by a link in the middle of the name either.
pub fn outgoing(cli: &str, name: &str) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        bail!("no file named");
    }
    let root = outbox(cli);
    let rel = Path::new(name);

    if rel.is_absolute() {
        bail!(
            "only files inside the outbox can be sent — give a name relative to it, not an \
             absolute path. Put the file in {} first.",
            crate::paths::simplified(&root).display()
        );
    }
    if rel.components().any(|c| matches!(c, Component::ParentDir)) {
        bail!("`..` is not allowed in a file name");
    }

    let candidate = root.join(rel);
    let real = std::fs::canonicalize(&candidate)
        .with_context(|| format!("no such file in the outbox: {name}"))?;
    // Canonicalize the root too: on macOS /var is itself a symlink to /private/var, so a
    // literal prefix test against the un-canonicalized root fails for every real path.
    let real_root = std::fs::canonicalize(&root).unwrap_or(root);
    if !real.starts_with(&real_root) {
        bail!("`{name}` resolves outside the outbox (a symlink out is not a way in)");
    }
    if !real.is_file() {
        bail!("`{name}` is not a file");
    }
    Ok(real)
}

/// Write received bytes into the quarantine, under a name WE choose.
///
/// `suggested` is the sender's idea of a filename and is treated as hostile: only its
/// extension is kept, and only when that extension is a short, plain-alphanumeric string.
/// The stem is ours (`id`), so a peer cannot pick the path, collide with an existing file,
/// smuggle separators, or hand us `.command`-style names that a double-click would run.
pub fn store(cli: &str, id: &str, suggested: Option<&str>, bytes: &[u8]) -> Result<PathBuf> {
    if bytes.len() as u64 > MAX_BYTES {
        bail!(
            "the file is {} MB; the limit is {} MB",
            bytes.len() / 1_048_576,
            MAX_BYTES / 1_048_576
        );
    }
    let ext = suggested
        .and_then(|s| Path::new(s).extension().and_then(|e| e.to_str()))
        .map(str::to_ascii_lowercase)
        .filter(|e| e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "bin".to_string());

    let stem: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(48)
        .collect();
    let stem = if stem.is_empty() { "file".to_string() } else { stem };

    let dir = inbox(cli);
    let path = dir.join(format!("{stem}.{ext}"));
    crate::store::atomic_write(&path, bytes)
        .with_context(|| format!("cannot write {}", path.display()))?;
    // Never executable: a received file is data, and nothing we write should be one
    // double-click away from running.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ENV_LOCK as LOCK;

    /// Point the whole module at a scratch dir. `CLATCH_DATA_DIR` is process-global and
    /// these functions read it, so the tests must take turns — the caller BINDS the
    /// returned guard, which is what keeps the turn for the whole test. (Dropping it
    /// inside this helper is exactly the race that made three of these flap.)
    fn scratch(name: &str) -> (std::sync::MutexGuard<'static, ()>, String, PathBuf) {
        let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("clappkit-media-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CLATCH_DATA_DIR", &dir);
        (g, format!("t{name}"), dir)
    }

    #[test]
    fn a_plain_name_in_the_outbox_resolves() {
        let (_g, cli, _d) = scratch("plain");
        std::fs::write(outbox(&cli).join("shot.png"), b"x").unwrap();
        assert!(outgoing(&cli, "shot.png").is_ok());
    }

    #[test]
    fn an_absolute_path_is_refused() {
        let (_g, cli, _d) = scratch("abs");
        let err = outgoing(&cli, "/etc/hosts").unwrap_err().to_string();
        assert!(err.contains("outbox"), "the refusal must say why: {err}");
    }

    #[test]
    fn traversal_is_refused() {
        let (_g, cli, _d) = scratch("trav");
        assert!(outgoing(&cli, "../../secret.txt").is_err());
        assert!(outgoing(&cli, "sub/../../secret.txt").is_err());
    }

    /// The one that matters: a link inside the outbox pointing at a real secret outside it.
    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_outbox_is_refused() {
        let (_g, cli, d) = scratch("link");
        let secret = d.join("id_rsa");
        std::fs::write(&secret, b"PRIVATE KEY").unwrap();
        std::os::unix::fs::symlink(&secret, outbox(&cli).join("innocent.txt")).unwrap();
        let err = outgoing(&cli, "innocent.txt").unwrap_err().to_string();
        assert!(err.contains("outside the outbox"), "got: {err}");
    }

    #[test]
    fn a_received_name_cannot_choose_its_path() {
        let (_g, cli, _d) = scratch("store");
        // A hostile "filename": separators, traversal, and an executable extension.
        let p = store(&cli, "msg-1", Some("../../../evil.command"), b"data").unwrap();
        assert_eq!(p.parent().unwrap(), inbox(&cli).as_path());
        assert_eq!(p.file_name().unwrap(), "msg-1.command".as_ref() as &std::ffi::OsStr);
        assert!(p.starts_with(inbox(&cli)), "must land in the quarantine");
    }

    #[test]
    fn a_weird_extension_falls_back_rather_than_being_trusted() {
        let (_g, cli, _d) = scratch("ext");
        let p = store(&cli, "m2", Some("x.this/is/not/an/ext"), b"d").unwrap();
        assert_eq!(p.extension().unwrap(), "bin");
    }

    #[test]
    fn an_oversized_file_is_refused_before_it_is_written() {
        let (_g, cli, _d) = scratch("big");
        let huge = vec![0u8; (MAX_BYTES + 1) as usize];
        assert!(store(&cli, "m3", Some("a.bin"), &huge).is_err());
    }

    #[test]
    fn kinds_are_picked_from_the_extension() {
        assert_eq!(Kind::of(Path::new("a.MP4")), Kind::Video);
        assert_eq!(Kind::of(Path::new("a.jpeg")), Kind::Image);
        assert_eq!(Kind::of(Path::new("a.opus")), Kind::Audio);
        assert_eq!(Kind::of(Path::new("a.zip")), Kind::Document);
        assert_eq!(Kind::of(Path::new("noext")), Kind::Document);
    }
}
