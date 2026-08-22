# Publishing to the Arfium marketplace

Everything a publisher has to know, in one page.

There are three things you can publish: a **clapp** and a **cli**, which live in your own
GitHub repository, and a **skill**, which is one Markdown file. Nothing is uploaded to the
marketplace except your store pictures.

---

## A clapp

### 1. Your repository keeps the code

You already build, tag and attach assets to a GitHub release. Keep doing exactly that. The
marketplace stores a **pointer** to your repository, not your bytes — Clatch downloads the
release from GitHub when somebody installs.

Requirements:

- The repository is **public**.
- It has at least one **published release**.
- That release has a `.clapp` asset for each platform you support, named
  `anything-<os>-<arch>.clapp` — for example `com.acme.notes-macos-arm64.clapp`.

### 2. Your `clatch.json` describes the app

It lives at the root of the `.clapp`. This is the file Clatch reads.

```jsonc
{
  "manifestVersion": 1,
  "id": "com.acme.notes",
  "name": "Notes",
  "description": "Play chess against your agent.",
  "version": "0.1.0",
  "protocol": 2,
  "icon": "assets/icon.png",
  "banner": "assets/banner.png",
  "about": "Longer text for the store page.",
  "tags": ["game", "chess"],
  "photos": ["assets/1.png", "assets/2.png"],
  "launch": { "macos": "bin/notes", "windows": "bin/notes.exe" },
  "connector": {
    "cli": "notes",
    "commands": [{ "name": "move", "about": "play a move (SAN)" }]
  }
}
```

**Required:** `manifestVersion`, `id`, `name`, `description`, `version`, `protocol`, `launch`
(at least one OS), `connector.cli`.
**Optional:** `icon`, `banner`, `about`, `tags`, `photos`, `connector.commands`,
`connector.signals`.

The operating systems in `launch` **are** the platforms the store shows. There is no separate
platforms field, and it cannot disagree with reality.

### 3. Your pictures go to the marketplace

These are for the store page, and they are uploaded separately — not read out of your
package. That way fixing a banner does not mean cutting a release, and cutting a release does
not silently change a store page nobody reviewed.

| | required | size | format | max |
|---|---|---|---|---|
| icon | **yes** | square, 512–1024 px | PNG, WebP | 512 KB |
| banner | no | 1600 × 500 | PNG, JPEG, WebP | 2 MB |
| photos | no, up to 4 | up to 1920 × 1080 | PNG, JPEG, WebP | 2 MB each |

No SVG — it can carry scripts and the store page draws it in a browser. No GIF.

One slot, one call, **raw bytes** as the body — not multipart, not base64, not JSON:

```
PUT /v1/publisher/elements/{id}/assets/{slot}
Content-Type: image/png
<the file, as-is>
```

`{slot}` is one of `icon`, `banner`, `photo1`, `photo2`, `photo3`, `photo4`. A slot holds
one picture: uploading again replaces what is there, so fixing a banner is one call and
nothing else moves. Photo order is the slot number.

```sh
curl -X PUT --data-binary @icon.png -H 'Content-Type: image/png' \
  https://…/v1/publisher/elements/com.acme.notes/assets/icon
```

**These are not the manifest's `icon` / `banner` / `photos`.** Those live inside the depot,
are read at install, and carry their own limits ([`format.md`](format.md) § Picture limits). The names are the same and the numbers are not: a store page is reviewed and can be
fixed without cutting a release, so it gets its own set. Do not copy one table into the
other.

### 4. Submit

Give the marketplace your repository. That is all it needs.

```
POST /v1/publisher/github
{ "id": "com.acme.notes", "repo": "acme/notes-clapp", "kind": "clapp",
  "name": "Notes", "summary": "Notes your agent can read and write." }
```

`kind` is `clapp` or `cli`. **A cli publishes the same way** — same repository pointer, same
`.clapp` assets per platform, same pictures. What differs is the package inside: no `launch`,
no signals, and its own auth verbs ([`format.md`](format.md) § What each type may
declare). It gets a store page like anything else.

You get back a **state**:

| | what it means | what to do |
|---|---|---|
| `review` | a human here will look at it | wait |
| `published` | it is live | nothing |
| `queued` | GitHub has not answered us yet | wait; we retry |
| `rejected` | something is wrong | read the reason, fix it, refresh |

A rejection always tells you why.

### 5. When you cut a new release

Send one signal. The marketplace re-reads your version, your star count and your platforms.

```
POST /v1/publisher/elements/{id}/refresh
```

It does not touch your store page.

### What you can change later

Everything **except your repository address**. That is frozen for the life of the element: a
listing that was reviewed and could then point somewhere else was never really reviewed.

If you need a different repository, publish a new element.

---

## A skill

One Markdown file. No package, no repository, no release.

```markdown
---
name: writing-style
description: How we write here.
tags: [writing, docs]
---

The body of the skill.
```

- `name` — **required**, lowercase letters, digits and dashes. This is its identity.
- `description` — **required**, the one line an agent reads to decide whether to use it.
- `tags` — optional.

**At most 200 KB**, which is roughly 50 000 tokens. A skill gets loaded into a context window;
something that eats a quarter of it before the work starts is a book, not a skill.

```
PUT /v1/publisher/skills
{ "id": "com.arfium.writing-style", "body": "---\nname: ...\n" }
```

Sending it again **replaces** it. A skill is named rather than versioned, so fixing a typo is
an edit, not a release.

---

## Rules that apply to both

1. **You must be a publisher.** Ask an admin, or redeem an invite code.
2. **The id is reverse-DNS** (`com.yourname.thing`) and is yours for good.
3. **Aggregates only, both ways.** You see how many accounts installed and voted. You never
   see which accounts, and nobody sees your repository's private anything.
4. **Votes come from Clatch accounts**, not GitHub. Somebody with no GitHub account can still
   vote on your clapp. Your GitHub stars are shown beside that number, never instead of it.
