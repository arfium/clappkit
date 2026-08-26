# The marks: icon and banner

[`format.md`](format.md#picture-limits) fixes the **format** of `assets/icon.png`: square
PNG, 512–1024 px, ≤ 1 MiB. This fixes the **design**, because icons sit side by side in
the library — if one fills its tile and another floats at 70%, the shelf looks broken.

## The rules

1. **Author at 1024×1024 RGBA**, and keep an editable source beside the PNG
   (`assets/icon.svg`, or a render script). The mark is regenerated, never hand-traced.

2. **Tiled icons are full-bleed.** The rounded tile fills the whole canvas; only the
   corners are transparent. Radius is `0.225 × side` (≈230 px at 1024) — Apple's own
   ratio, and the **Dock** is where your corner is actually seen. The Clatch library
   re-rounds every tile to `0.26 × side` in CSS, icon and letter-tile alike, so the shelf
   is uniform by the launcher's doing rather than yours. This is the default for any app
   with a coloured background.

3. **Transparent icons maximise the glyph** — no tile, mark at ~95–98% of canvas height,
   centred. A tall narrow mark won't fill the width; match the *height* so it carries the
   same weight as a full-bleed tile.

4. **The mark reads on light and dark.** A mark on a saturated tile is safe. A transparent
   one needs its own definition — a crisp outline, or a soft shadow.

5. **Use the real mark, and don't design one yourself.** For a real product take the
   official glyph ([simple-icons](https://simpleicons.org)); otherwise take a permissively
   licensed one ([Lucide](https://lucide.dev), ISC) and credit it in
   `THIRD_PARTY_NOTICES.md`. Never approximate a logo by hand, and never stand in a system
   symbol for a brand mark — it is close enough to read as a bug.

Measure the fill rather than trusting your eye:

```sh
python3 -c "
from PIL import Image
im = Image.open('assets/icon.png').convert('RGBA'); W,H = im.size; b = im.getbbox()
print(f'{100*(b[2]-b[0])//W}% x {100*(b[3]-b[1])//H}%')"   # ~100% tile · ~95%+ glyph height
```

## The desktop is a second standard

A full-bleed tile is right in the library and **wrong in the Dock**, where every icon is
inset and yours would tower over its neighbours. `clappkit::icon::dock_icon` insets it to
~80% at runtime, so one PNG serves both.

That only helps once the OS has an icon to show at all:

- **macOS** — a bare executable has no icon identity, so the Dock falls back to a generic
  terminal tile. `scripts/package.sh` puts the binary in a real `.app` bundle with an
  `.icns`, which is what actually fixes it. Don't demote the activation policy on a normal
  launch: that rebuilds the tile and the icon flickers back to the fallback.
- **Windows** — `src-tauri/icons/icon.ico` is compiled into the executable as a resource.
  Derive it from the same `assets/icon.png` so the two cannot drift.

## The banner is a strip, not a picture

The bounds are in [`format.md`](format.md) § Picture limits — 215:32, at least 3440×512.
This is what to put inside them.

**The library draws it 128px tall.** Everything else follows from that. A 4px hairline at
3440 is one pixel at 860 and gone at 128, so nothing is thinner than **6px** at full size
and most strokes are 12–18. Detail you cannot see is worse than empty space: it turns to
noise.

**The left 40% is not yours.** The launcher lays a left-dark scrim there and prints the
app's name over it in white. Anything drawn under it is either invisible or fighting the
one thing that has to be read. Keep the motif **right of centre** and leave the left with
ground, or with something quiet enough to be a texture.

**Draw what the app does, not what it is called.** A route, a road with one waypoint
ringed, a checkerboard with three pieces, a film strip: somebody scrolling a shelf reads
the picture before the name, and a logo repeated at banner size tells them nothing the
icon did not already say.

**Use the icon's own colours.** Sample them rather than picking new ones — the banner and
the icon sit inches apart, and a second palette makes them look like two products.

`template-clapp/scripts/render-banner.py` draws every banner in this family from one file:
a shared grammar, one motif per app. That is deliberate. Sixteen banners drawn separately
become sixteen unrelated pictures; drawn together they become a shelf.

## `pkg/` is a copy

`scripts/package.sh` copies `assets/icon.png` and `assets/banner.png` into the depot; it
does not update them itself.
`pkg/` and `*.clapp` are build outputs, gitignored everywhere, and the committed
`assets/icon.png` is the truth. If the library still shows the old mark, you did not
repackage.

## Checklist

- [ ] 1024×1024 RGBA, ≤ 1 MiB
- [ ] an editable source exists next to it
- [ ] fill measured: ~100% tile, or ~95%+ glyph height
- [ ] reads on light and dark
- [ ] a real or properly licensed mark, credited if required
- [ ] `.ico` re-derived, and the depot repackaged

And the banner:

- [ ] 3440×512, 215:32, ≤ 2 MiB
- [ ] nothing thinner than 6px, checked at 128px tall — not at full size
- [ ] the left 40% carries nothing that has to be read
- [ ] the motif says what the app DOES
- [ ] the palette is the icon's
