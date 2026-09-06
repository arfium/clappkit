#!/usr/bin/env bash
# The documents in docs/ are the format contract two other repositories build against, so
# they are held, not trusted to habit. This gate refuses the drift a reader would otherwise
# be the first to find: a broken link, a document missing from the index, a noun defined in
# two places, a section citation that names a heading nobody kept.
#
# Deliberately weak where it must be: it checks the SHAPE of the documents, never that a
# written rule is true — no script can check the second, and one that pretended to would be
# worse than none. It frees a reader to judge the prose.
set -euo pipefail
cd "$(dirname "$0")/.."

# ── Links resolve ────────────────────────────────────────────────────────────────────
# A link to a file that is not there sends the next reader nowhere. Resolved relative to
# the linking file's own directory, which is what a click does. Links that leave the repo
# (into ../../clatch-server, say) are checked only when that sibling is checked out.
python3 - "$PWD" <<'LINKCHECK' || exit 1
import os, re, sys
root = os.path.realpath(sys.argv[1])
broken, skipped = [], 0
for dirpath, _, files in os.walk("docs"):
    for name in files:
        if not name.endswith(".md"):
            continue
        path = os.path.join(dirpath, name)
        text = open(path, encoding="utf-8").read()
        for rel in sorted(set(re.findall(r"\]\(([^):#]+\.md)", text))):
            target = os.path.realpath(os.path.join(dirpath, rel))
            if os.path.isfile(target):
                continue
            inside = target == root or target.startswith(root + os.sep)
            if inside or os.path.isdir(os.path.dirname(target)):
                broken.append(f"  {path} -> {rel}")
            else:
                skipped += 1
if broken:
    print("these documents link to files that do not exist:", file=sys.stderr)
    print("\n".join(sorted(set(broken))), file=sys.stderr)
    sys.exit(1)
if skipped:
    print(f"docs: {skipped} link(s) into sibling repositories not checked out — skipped")
LINKCHECK
printf 'docs: links resolve\n'

# ── Consistency ──────────────────────────────────────────────────────────────────────
# Each class below is a drift a person caught by reading, twice. A person is the wrong
# instrument for "is this written somewhere else too"; these are mechanical, so the gate
# holds them.
python3 - <<'PYEOF' || exit 1
import glob, os, re, sys

problems = []
docs = sorted(glob.glob("docs/*.md"))
text = {p: open(p, encoding="utf-8").read() for p in docs}

# 1. A noun defined in two documents is two nouns waiting to disagree. The glossary form is
#    a bold term opening a line, followed by an em-dash; taxonomy.md owns every one.
defined = {}
for p, s in text.items():
    for m in re.finditer(r"^\*\*([A-Za-z][A-Za-z ()'`/:.-]{1,40}?)\*\*\s+—", s, re.M):
        defined.setdefault(m.group(1).strip().lower(), []).append(p)
for term, where in sorted(defined.items()):
    if len(set(where)) > 1:
        problems.append(f"`{term}` is defined in {', '.join(sorted(set(where)))} — taxonomy.md owns definitions")

# 2. A heading written twice in one file, or one that promises a body and delivers the next
#    heading. A heading followed by a DEEPER one is a divider and wants no prose of its own.
for p, s in text.items():
    lines = s.split("\n")
    seen, incode = {}, False
    for i, l in enumerate(lines):
        if l.startswith("```"):
            incode = not incode
        if incode or not l.startswith("#"):
            continue
        title = l.lstrip("#").strip()
        if title in seen:
            problems.append(f"{p}: heading {title!r} appears at lines {seen[title]} and {i+1}")
        seen[title] = i + 1
        depth = len(l) - len(l.lstrip("#"))
        rest = [x for x in lines[i + 1:] if x.strip()]
        empty = not rest or (rest[0].startswith("#") and len(rest[0]) - len(rest[0].lstrip("#")) <= depth)
        if empty:
            problems.append(f"{p}:{i+1}: heading {title!r} has no body")

# 3. A table split by a paragraph: rows, a gap of prose, then more rows. The renderer drops
#    the tail, silently.
for p, s in text.items():
    lines = s.split("\n")
    for i, l in enumerate(lines):
        if not l.startswith("|"):
            continue
        j = i + 1
        while j < len(lines) and not lines[j].startswith("|"):
            if lines[j].strip():
                break
            j += 1
        else:
            continue
        if j < len(lines) and lines[j].startswith("|") and lines[j - 1].strip() and not lines[j - 1].startswith("|"):
            problems.append(f"{p}:{j+1}: a table continues after a paragraph — the rows below it will not render")

# 4. A section citation must name a heading that exists. Renaming a section silently strips
#    every "§ Foo" pointing at it.
heads = {}
for p, s in text.items():
    incode = False
    hs = set()
    for l in s.split("\n"):
        if l.startswith("```"):
            incode = not incode
        elif not incode and l.startswith("#"):
            hs.add(l.lstrip("#").strip().lower())
    heads[os.path.basename(p)] = hs
for p, s in text.items():
    for m in re.finditer(r"([a-z-]+\.md)\)\s*§\s*([0-9]+[.．]?\s*[A-Za-z][A-Za-z0-9 ,'&/-]+)", s):
        doc = m.group(1)
        sec = m.group(2).strip().rstrip(".,;:").lower()
        if doc not in heads:
            continue
        if not any(h == sec or h.startswith(sec) or sec.startswith(h) for h in heads[doc]):
            problems.append(f"{p}: cites {doc} § {m.group(2).strip()} — no such heading")

# 5. A document's purpose is written once, in the index. A file that restates it at its head
#    is the same fact in two places. And the title is a short uppercase name — nothing else.
for p, s in text.items():
    head = s.split("\n")[:8]
    for field in ("PURPOSE", "SCOPE", "AUTHORITY"):
        if any(l.startswith(f"**{field}.**") for l in head):
            problems.append(f"{p}: {field} at the head — index.md § 2 is where a document is described")
    if not head or not re.match(r"^# [A-Z][A-Z ]+$", head[0]):
        problems.append(f"{p}: the title must be a short uppercase name — got {head[0] if head else '(empty)'!r}")

# 6. One H1 per file: it is the title. Code fences are skipped — a `# comment` is not one.
for p, s in text.items():
    incode, h1 = False, 0
    for l in s.split("\n"):
        if l.startswith("```"):
            incode = not incode
        elif not incode and re.match(r"^# ", l):
            h1 += 1
    if h1 != 1:
        problems.append(f"{p}: {h1} level-one headings — the title is the only one")

# 7. The index is the map. Every document must be named in it and have a row in § 2; a name
#    it keeps after a file is gone is a link that used to work.
index = text.get("docs/index.md", "")
for p in docs:
    b = os.path.basename(p)
    if b == "index.md":
        continue
    if not re.search(rf"\[{re.escape(b)}\]\({re.escape(b)}\)\s*\|", index):
        problems.append(f"{b} has no row in docs/index.md § 2")
for m in re.finditer(r"\[([a-z-]+\.md)\]", index):
    if not os.path.exists(os.path.join("docs", m.group(1))):
        problems.append(f"docs/index.md names {m.group(1)}, which does not exist")


# 8. Every manifest field format.md's example shows must be a documented field — a row in
#    one of format.md's tables. The example is what a person copies; a key in it that no
#    table defines is a field nobody specified.
fmt = text.get("docs/format.md", "")
import re as _re
# top-level keys in the first jsonc block (two-space indent under the opening brace)
mblock = _re.search(r"```jsonc\n(.*?)```", fmt, _re.S)
example_keys = set()
if mblock:
    for ln in mblock.group(1).split("\n"):
        m = _re.match(r'  "([a-zA-Z][a-zA-Z0-9]*)"\s*:', ln)
        if m:
            example_keys.add(m.group(1))
# a field is documented either by a table row (| `field` | ...) or by its own numbered
# section heading (## N. `field`) — objects like launch and connector get a whole section.
documented = set(_re.findall(r"^\| `([a-zA-Z][a-zA-Z0-9]*)`", fmt, _re.M))
documented |= set(_re.findall(r"^#+ \d+\. `([a-zA-Z][a-zA-Z0-9]*)`", fmt, _re.M))
for k in sorted(example_keys - documented):
    problems.append(f"docs/format.md: the manifest example shows `{k}`, which no field table or section documents")

# 9. No em (U+2014) or en (U+2013) dash in anything we author. The sibling trees hold none
#    (clatch's quality_gate.sh enforces the same), and a hyphen, colon or reword always says
#    it in ASCII. Built from code points so this checker holds neither dash itself.
_em, _en = chr(0x2014), chr(0x2013)
for p, s in text.items():
    for i, ln in enumerate(s.split("\n"), 1):
        if _em in ln or _en in ln:
            problems.append(f"{p}:{i}: em/en dash present, use a plain hyphen, colon, or reword")

if problems:
    print("docs: consistency", file=sys.stderr)
    for x in sorted(set(problems)):
        print("  " + x, file=sys.stderr)
    sys.exit(1)
print("docs: titles, headings, tables, citations, manifest fields and the index all hold")
PYEOF
