//! Local images, for a webview that cannot open them.
//!
//! Clatch's roster hands an app an agent's avatar as an **absolute file path** on this
//! machine — never bytes (docs/protocol.md § the agent roster). A Tauri webview cannot
//! load a `file://` URL, so the one bridge is: read the file in the core, hand the front
//! end a `data:` URI. chess and telegram wrote that function character for character
//! identically, and telegram wrote a third, subtly different copy of the extension→MIME
//! table for its Bot API avatar upload.
//!
//! This module is that one table and that one bridge — and it is **tauri-free on
//! purpose**, so the Tauri `#[tauri::command] fn asset` shim is three lines in the app
//! and the same logic is available to a headless clapp. The base64 encoder is 15 lines
//! of std rather than a dependency, which also lets whatsapp drop the `base64` crate it
//! declares and never uses.

/// The image MIME for a path, by extension: `jpg`/`jpeg` · `gif` · `webp` · `png`.
///
/// PNG is the fallback for anything unrecognised because an agent avatar is a transparent
/// mark far more often than it is a photo, and a JPEG guess flattens the alpha the moment
/// it is re-encoded (telegram's third copy of this table defaulted to jpeg — this is the
/// arm that disagreed).
pub fn image_mime(path: &str) -> &'static str {
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

/// A filename to present an upload under (`avatar.png`, `avatar.jpg`, …), derived from
/// the same table as [`image_mime`] so the two can never disagree — Telegram's
/// `setMyProfilePhoto` needs a name and a MIME that match.
pub fn image_filename(path: &str) -> String {
    let ext = match image_mime(path) {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    };
    format!("avatar.{ext}")
}

/// Read a local image and return it as a `data:` URI the webview can put in an `<img>`.
/// `None` when the file is unreadable — the caller falls back to a monogram, which is
/// what the front ends already do.
pub fn data_uri(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(data_uri_from(image_mime(path), &bytes))
}

/// The `data:` URI for bytes whose MIME you already know.
pub fn data_uri_from(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64_encode(bytes))
}

/// Read an image for upload: its bytes, MIME and a matching filename. The one place a
/// clapp needs all three (Telegram's multipart avatar upload); no transcoding, the bytes
/// go up as authored.
pub fn read_image(path: &str) -> Option<(Vec<u8>, &'static str, String)> {
    let bytes = std::fs::read(path).ok()?;
    Some((bytes, image_mime(path), image_filename(path)))
}

/// Standard base64 (RFC 4648 §4, with padding) — the alphabet a `data:` URI wants.
/// Hand-rolled so the core takes no dependency for 15 lines of arithmetic; verified
/// against the RFC's own test vectors below.
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_vectors() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(input.as_bytes()), want, "input {input:?}");
        }
    }

    #[test]
    fn every_byte_value_encodes_to_the_standard_alphabet_and_the_right_length() {
        let all: Vec<u8> = (0..=255u8).collect();
        let enc = base64_encode(&all);
        assert_eq!(enc.len(), all.len().div_ceil(3) * 4);
        assert!(
            enc.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'/' || c == b'='),
            "non-standard character in {enc}"
        );
        // 256 bytes = 85 full groups + 1 leftover byte → exactly two pad characters.
        assert!(enc.ends_with("=="), "{enc}");
    }

    #[test]
    fn png_is_the_default_mime_not_jpeg() {
        assert_eq!(image_mime("/a/b/avatar.png"), "image/png");
        assert_eq!(image_mime("/a/b/avatar"), "image/png");
        assert_eq!(image_mime("/a/b/avatar.tiff"), "image/png");
    }

    #[test]
    fn mime_is_case_insensitive_and_covers_the_table() {
        assert_eq!(image_mime("a.JPG"), "image/jpeg");
        assert_eq!(image_mime("a.jpeg"), "image/jpeg");
        assert_eq!(image_mime("a.GIF"), "image/gif");
        assert_eq!(image_mime("a.WebP"), "image/webp");
    }

    #[test]
    fn filename_and_mime_always_agree() {
        for (path, mime, name) in [
            ("x.jpg", "image/jpeg", "avatar.jpg"),
            ("x.gif", "image/gif", "avatar.gif"),
            ("x.webp", "image/webp", "avatar.webp"),
            ("x.bin", "image/png", "avatar.png"),
        ] {
            assert_eq!(image_mime(path), mime);
            assert_eq!(image_filename(path), name);
        }
    }

    #[test]
    fn data_uri_reads_a_real_file_and_is_none_for_a_missing_one() {
        let dir = std::env::temp_dir().join(format!("clappkit-asset-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("avatar.gif");
        std::fs::write(&p, b"foobar").unwrap();
        assert_eq!(
            data_uri(p.to_str().unwrap()),
            Some("data:image/gif;base64,Zm9vYmFy".to_string())
        );
        assert_eq!(data_uri(dir.join("nope.png").to_str().unwrap()), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
