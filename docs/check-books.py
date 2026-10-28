#!/usr/bin/env python3
"""Consistency checks for the Kagari books (no dependencies; run from anywhere).

- every SUMMARY entry exists and every page is listed in SUMMARY;
- relative links and `#anchors` resolve (anchors computed like mdBook, explicit `{#id}`
  and inline `<a id>` included), also across books;
- `ja` mirrors `en` (same files, same heading ids, byte-identical code blocks) and every
  `ja` heading carries an explicit `{#id}`;
- every included diagram exists and is preceded by its hidden mermaid-ish comment.

Exit status 1 when a problem is found.
"""
import collections
import os
import re
import sys

DOCS = os.path.dirname(os.path.abspath(__file__))
BOOKS = ['reference/en', 'reference/ja', 'std/en', 'std/ja', 'dev']
MIRRORED = ['reference', 'std']
FENCE = re.compile(r'^\s*(```+|~~~+)')
LINK = re.compile(r'!?\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)')
INCLUDE = re.compile(r'(?<!\\)\{\{#include ([^}]+\.svg)\}\}')

problems = collections.defaultdict(list)


def split_code(text):
    """The lines outside code blocks (code lines blanked) and the list of code blocks."""
    out, blocks, cur, fence = [], [], [], None
    for line in text.split('\n'):
        m = FENCE.match(line)
        if fence is None:
            if m:
                fence, cur = m.group(1), [line]
                out.append('')
            else:
                out.append(line)
            continue
        cur.append(line)
        out.append('')
        if m and line.strip() == fence[0] * len(line.strip()) and len(line.strip()) >= len(fence):
            blocks.append('\n'.join(cur))
            fence = None
    return out, blocks


def slug(text):
    text = re.sub(r'\[([^\]]*)\]\([^)]*\)', r'\1', text)
    parts = re.split(r'(`[^`]*`)', text)
    text = ''.join(p[1:-1] if p.startswith('`') else re.sub(r'<[^>]+>', '', p) for p in parts)
    return ''.join(c if (c.isalnum() or c in '_-') else '-' if c == ' ' else ''
                   for c in text.strip().lower())


cache = {}


def headings(path):
    """(set of anchor ids, [(level, id, explicit)] of the headings) of a Markdown file."""
    if path in cache:
        return cache[path]
    lines, _ = split_code(open(path, encoding='utf-8').read())
    ids, seq, counts = set(), [], collections.Counter()
    for line in lines:
        m = re.match(r'^(#{1,6})\s+(.*?)\s*$', line)
        if m:
            text = m.group(2)
            explicit = re.search(r'\{#([^}\s]+)[^}]*\}\s*$', text)
            if explicit:
                hid = explicit.group(1)
            else:
                base = slug(text)
                hid = f'{base}-{counts[base]}' if counts[base] else base
                counts[base] += 1
            ids.add(hid)
            seq.append((len(m.group(1)), hid, bool(explicit)))
        ids.update(re.findall(r'<(?:a|span|div)\s+[^>]*id="([^"]+)"', line))
    cache[path] = (ids, seq)
    return cache[path]


def check_book(book):
    src = os.path.join(DOCS, book, 'src')
    summary = open(os.path.join(src, 'SUMMARY.md'), encoding='utf-8').read()
    listed = re.findall(r'\]\(\./([^)]+)\)', summary)
    for page in listed:
        if not os.path.exists(os.path.join(src, page)):
            problems['missing page'].append(f'{book}: {page}')
    for dirpath, _, files in os.walk(src):
        for name in files:
            if not name.endswith('.md'):
                continue
            path = os.path.join(dirpath, name)
            rel = os.path.relpath(path, src)
            if rel != 'SUMMARY.md' and rel not in listed:
                problems['page not in SUMMARY'].append(f'{book}: {rel}')
            text = open(path, encoding='utf-8').read()
            body = re.sub(r'`[^`\n]*`', '', '\n'.join(split_code(text)[0]))
            for m in LINK.finditer(body):
                target = m.group(1)
                if re.match(r'^[a-z][a-z0-9+.-]*:', target):
                    continue
                file, _, anchor = target.partition('#')
                dest = path if not file else os.path.normpath(os.path.join(dirpath, file))
                if file and not os.path.exists(dest):
                    problems['broken link'].append(f'{book}/{rel}: {target}')
                elif anchor and dest.endswith('.md') and anchor not in headings(dest)[0]:
                    problems['broken anchor'].append(f'{book}/{rel}: {target}')
            for m in INCLUDE.finditer(text):
                if not os.path.exists(os.path.normpath(os.path.join(dirpath, m.group(1).strip()))):
                    problems['missing diagram'].append(f'{book}/{rel}: {m.group(1)}')
                before = text[:m.start()].rstrip()
                comment = before.rfind('<!-- diagram')
                if not before.endswith('-->') or comment < 0 or '-->' in before[comment:-3]:
                    problems['diagram without mermaid-ish comment'].append(f'{book}/{rel}: {m.group(1)}')


def check_mirror(book):
    en, ja = (os.path.join(DOCS, book, lang, 'src') for lang in ('en', 'ja'))

    def files(root):
        return {os.path.relpath(os.path.join(d, f), root) for d, _, fs in os.walk(root) for f in fs}

    fe, fj = files(en), files(ja)
    for f in sorted(fe ^ fj):
        problems['not mirrored'].append(f'{book}: {f} only in {"en" if f in fe else "ja"}')
    for f in sorted(fe & fj):
        if not f.endswith('.md') or f == 'SUMMARY.md':
            continue
        he = [(lvl, hid) for lvl, hid, _ in headings(os.path.join(en, f))[1]]
        hj_full = headings(os.path.join(ja, f))[1]
        hj = [(lvl, hid) for lvl, hid, _ in hj_full]
        if he != hj:
            first = next((i for i, (a, b) in enumerate(zip(he, hj)) if a != b), min(len(he), len(hj)))
            problems['headings differ'].append(
                f'{book}/{f}: en {he[first:first + 1]} vs ja {hj[first:first + 1]}')
        for _, hid, explicit in hj_full:
            if not explicit:
                problems['ja heading without {#id}'].append(f'{book}/{f}: {hid}')
        ce = split_code(open(os.path.join(en, f), encoding='utf-8').read())[1]
        cj = split_code(open(os.path.join(ja, f), encoding='utf-8').read())[1]
        if ce != cj:
            problems['code blocks differ'].append(f'{book}/{f}: en {len(ce)} blocks, ja {len(cj)}')


for b in BOOKS:
    check_book(b)
for b in MIRRORED:
    check_mirror(b)

for kind, items in problems.items():
    print(f'{kind} ({len(items)})')
    for item in items:
        print(f'  {item}')
if problems:
    sys.exit(1)
print('books ok')
