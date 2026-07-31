//! The Dock/taskbar icon for a **bare-executable** clapp.
//!
//! A clapp ships as a lone binary (no `.app` bundle, no embedded resource fork) that
//! Clatch launches directly. With nothing to carry an icon, macOS falls back to the
//! generic terminal tile in the Dock. The fix is the same one the native Swift apps
//! used — set it at runtime: `NSApp.applicationIconImage = NSImage(data:)`. This module
//! is that one line, kept here so every clapp shares it (DRY) and clappkit's core stays
//! tauri-free — pure AppKit via the version-stable `cocoa`/`objc` crates, which coexist
//! happily with the `objc2` that tauri pulls in. (The `tauri` feature adds
//! [`crate::app::apply_icon`], which does the window/taskbar half on top of this one.)
//!
//! This module is itself the `icon` feature — on by default, but a headless clapp can turn
//! it off and stop compiling a PNG codec and AppKit for a Dock tile it has no use for.
//!
//! ## Full-bleed source, padded Dock icon
//!
//! `assets/icon.png` is authored **full-bleed** (the tile fills the whole canvas) because
//! that is the Clatch *library* house style — tiles on a shelf must all read at one size
//! (docs/ICONS.md). But the macOS **Dock** expects the opposite: native icons carry a
//! transparent margin (~10–20%) so the squircle sits at the standard Dock-grid size. Hand
//! the Dock a full-bleed PNG and it renders edge-to-edge — visibly bigger than every
//! native neighbour. So [`dock_icon`] insets a full-bleed tile to Apple's ~80% grid before
//! it becomes the Dock/taskbar icon, while the source PNG stays full-bleed for the shelf.
//! A mark that already carries its own margin (a transparent glyph like chess's pawn) is
//! left untouched.
//!
//! On Windows/Linux there is no process-wide icon to set the AppKit way; the app gives its
//! *window* an icon (`WebviewWindow::set_icon`) — pass it [`dock_icon`] too, for the same
//! sizing. So [`set_dock_icon`] is a deliberate no-op off macOS.

use image::{imageops, GenericImageView, ImageFormat, RgbaImage};
use std::io::Cursor;

/// Apple's continuous-rounded-rect icon grid: the tile is ~824/1024 ≈ 0.805 of the canvas,
/// centred, leaving a transparent margin. We target 0.80 so a full-bleed clapp tile sits
/// at the same size as the native apps beside it in the Dock.
const DOCK_FILL: f32 = 0.80;

/// A mark is treated as a full-bleed tile (and inset for the Dock) only when it fills at
/// least this fraction of the canvas in BOTH dimensions. A transparent glyph (chess's pawn
/// is ~66%×80%) stays below this and is left exactly as authored.
const FULL_BLEED: f32 = 0.90;

/// The Dock/taskbar form of an icon: a full-bleed tile inset to the macOS grid (~80% with
/// a transparent margin); anything that already carries its own margin is returned as-is.
/// On any decode/encode failure the input bytes are returned unchanged — a slightly-large
/// icon beats no icon.
pub fn dock_icon(png: &[u8]) -> Vec<u8> {
    pad_full_bleed(png).unwrap_or_else(|| png.to_vec())
}

fn pad_full_bleed(png: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(png).ok()?;
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }

    // Opaque-pixel bounding box → how much of the canvas the mark actually covers.
    let rgba = img.to_rgba8();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for (x, y, p) in rgba.enumerate_pixels() {
        if p[3] > 10 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return None; // fully transparent — nothing to do
    }
    let fill_w = (x1 - x0) as f32 / w as f32;
    let fill_h = (y1 - y0) as f32 / h as f32;
    if fill_w.min(fill_h) < FULL_BLEED {
        return None; // already carries a margin (a glyph) — leave it exactly as authored
    }

    // Full-bleed tile: scale the whole square down to the Dock grid, centre on transparent.
    let side = w.max(h);
    let target = ((side as f32) * DOCK_FILL).round() as u32;
    let scaled = img
        .resize(target, target, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut canvas = RgbaImage::new(side, side); // transparent
    let ox = ((side - scaled.width()) / 2) as i64;
    let oy = ((side - scaled.height()) / 2) as i64;
    imageops::overlay(&mut canvas, &scaled, ox, oy);

    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(canvas)
        .write_to(&mut out, ImageFormat::Png)
        .ok()?;
    Some(out.into_inner())
}

/// Point the macOS Dock at the app's own icon. `png` is the icon bytes — embed them with
/// `include_bytes!` so the bare executable carries its own mark. The bytes are run through
/// [`dock_icon`] first, so a full-bleed tile is inset to the native Dock size. **Call on
/// the main thread** (Tauri's `setup` closure is the main thread). A no-op off macOS.
#[cfg(target_os = "macos")]
pub fn set_dock_icon(png: &[u8]) {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSData;
    use objc::{class, msg_send, sel, sel_impl};

    let bytes = dock_icon(png);

    // SAFETY: standard AppKit object graph — NSData over the bytes (copied by
    // -initWithData:), an NSImage from it, then hand it to the shared NSApplication. All
    // on the main thread, per the doc contract. A nil image (undecodable bytes) is skipped.
    unsafe {
        let data: id = NSData::dataWithBytes_length_(
            nil,
            bytes.as_ptr() as *const std::os::raw::c_void,
            bytes.len() as u64,
        );
        let image: id = msg_send![class!(NSImage), alloc];
        let image: id = msg_send![image, initWithData: data];
        if image != nil {
            let app: id = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, setApplicationIconImage: image];
        }
    }
}

/// Off macOS the process has no Dock tile to set — the app icons its *window* instead
/// (with [`dock_icon`] bytes, for the same sizing).
#[cfg(not(target_os = "macos"))]
pub fn set_dock_icon(_png: &[u8]) {}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn png_of(img: &RgbaImage) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img.clone())
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// Opaque-pixel bbox fill (max of the two dims) of a PNG, as a fraction.
    fn fill(png: &[u8]) -> f32 {
        let rgba = image::load_from_memory(png).unwrap().to_rgba8();
        let (w, h) = rgba.dimensions();
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
        for (x, y, p) in rgba.enumerate_pixels() {
            if p[3] > 10 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
        (((x1 - x0) as f32 / w as f32).max((y1 - y0) as f32 / h as f32)).max(0.0)
    }

    #[test]
    fn full_bleed_tile_is_inset_to_the_dock_grid() {
        // A 512² fully-opaque tile (fill 100%, like clock/telegram/whatsapp/template).
        let tile = RgbaImage::from_pixel(512, 512, Rgba([20, 180, 90, 255]));
        let out = dock_icon(&png_of(&tile));
        let f = fill(&out);
        assert!((0.76..=0.84).contains(&f), "expected ~0.80 fill, got {f}");
    }

    #[test]
    fn a_glyph_that_already_has_margin_is_left_alone() {
        // A centered 300² mark on a 512² transparent canvas (fill ~0.586, like chess).
        let mut g = RgbaImage::new(512, 512);
        for y in 106..406 {
            for x in 106..406 {
                g.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        let before = png_of(&g);
        let out = dock_icon(&before);
        assert!(
            (fill(&out) - fill(&before)).abs() < 0.02,
            "a glyph with its own margin must pass through unchanged"
        );
    }
}
