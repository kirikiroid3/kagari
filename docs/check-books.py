#!/usr/bin/env python3
"""Consistency checks for the documentation (no dependencies; run from anywhere).

The documentation site (`docs/site`, en and ja):
- every page has a `title` and is listed in `docsNav` of `theme.config.json`, and every
  `docsNav` entry exists in both languages;
- relative links and `#anchors` resolve (anchors computed like mdBook, explicit `{#id}` and
  inline `<a id>` included; `docs/site/src/markdown.ts` gives the built pages the same ids);
- `ja` mirrors `en` (same files, same heading ids, byte-identical code blocks) and every `ja`
  heading carries an explicit `{#id}`;
- every included diagram exists and is preceded by its hidden mermaid-ish comment.

The Develop Guide (`docs/dev`, mdBook):
- every SUMMARY entry exists and every page is listed in SUMMARY;
- relative links, anchors and diagrams as above; links to the site
  (`https://kagari.suitsu31.club/...`) resolve to a page and anchor of the site sources.

Exit status 1 when a problem is found.
"""
import collections
import json
import os
import re
import sys

DOCS = os.path.dirname(os.path.abspath(__file__))
SITE = os.path.join(DOCS, 'site', 'src', 'content', 'docs')
SITE_URL = 'https://kagari.suitsu31.club'
LANGS = {'ja': '', 'en': '/en'}  # locale directory -> URL prefix (ja is the root locale)
DEV = os.path.join(DOCS, 'dev', 'src')
FENCE = re.compile(r'^\s*(```+|~~~+)')
LINK = re.compile(r'!?\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)')
INCLUDE = re.compile(r'(?<!\\)\{\{#include ([^}]+\.svg)\}\}')
FRONTMATTER = re.compile(r'\A---\n(.*?)\n---\n', re.S)

problems = collections.defaultdict(list)


def read(path):
    with open(path, encoding='utf-8') as f:
        return f.read()


def body(text):
    """The Markdown after the frontmatter, with the frontmatter lines blanked (line numbers kept)."""
    m = FRONTMATTER.match(text)
    return '\n' * m.group(0).count('\n') + text[m.end():] if m else text


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
    lines, _ = split_code(body(read(path)))
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


def site_page(url):
    """The source file of a site URL (`/en/std/screen/` -> `en/std/screen.md`), or None."""
    path = url.strip('/')
    lang = 'en' if path == 'en' or path.startswith('en/') else 'ja'
    rest = path[len('en'):].strip('/') if lang == 'en' else path
    candidates = [f'{rest}.md', f'{rest}/index.md'] if rest else ['index.md']
    for c in candidates:
        full = os.path.join(SITE, lang, c)
        if os.path.exists(full):
            return full
    return None


def check_links(label, path, text):
    """Relative links, anchors, site URLs and diagram includes of one Markdown file."""
    dirpath = os.path.dirname(path)
    lines = split_code(body(text))[0]
    stripped = re.sub(r'`[^`\n]*`', '', '\n'.join(lines))
    for m in LINK.finditer(stripped):
        target = m.group(1)
        if target.startswith(SITE_URL):
            url, _, anchor = target[len(SITE_URL):].partition('#')
            dest = site_page(url)
            if dest is None:
                problems['broken site link'].append(f'{label}: {target}')
            elif anchor and anchor not in headings(dest)[0]:
                problems['broken site anchor'].append(f'{label}: {target}')
            continue
        if re.match(r'^[a-z][a-z0-9+.-]*:', target):
            continue
        file, _, anchor = target.partition('#')
        dest = path if not file else os.path.normpath(os.path.join(dirpath, file))
        if file and not os.path.exists(dest):
            problems['broken link'].append(f'{label}: {target}')
        elif anchor and dest.endswith('.md') and anchor not in headings(dest)[0]:
            problems['broken anchor'].append(f'{label}: {target}')
    for m in INCLUDE.finditer(text):
        if not os.path.exists(os.path.normpath(os.path.join(dirpath, m.group(1).strip()))):
            problems['missing diagram'].append(f'{label}: {m.group(1)}')
        before = text[:m.start()].rstrip()
        comment = before.rfind('<!-- diagram')
        if not before.endswith('-->') or comment < 0 or '-->' in before[comment:-3]:
            problems['diagram without mermaid-ish comment'].append(f'{label}: {m.group(1)}')


def markdown_files(root):
    return sorted(os.path.relpath(os.path.join(d, f), root)
                  for d, _, fs in os.walk(root) for f in fs if f.endswith('.md'))


def nav_entries(items):
    for item in items:
        if isinstance(item, str):
            yield item
        elif 'items' in item:
            yield from nav_entries(item['items'])


def entry_id(rel):
    return re.sub(r'(^|/)index$', '', rel[:-len('.md')])


def check_site():
    config = json.loads(read(os.path.join(DOCS, 'site', 'theme.config.json')))
    listed = [e for section in config['docsNav'] for e in nav_entries(section['items'])]
    for lang in LANGS:
        root = os.path.join(SITE, lang)
        pages = {entry_id(rel): rel for rel in markdown_files(root)}
        for entry in listed:
            if entry not in pages:
                problems['docsNav entry without page'].append(f'{lang}: {entry}')
        for eid, rel in pages.items():
            label = f'site/{lang}/{rel}'
            path = os.path.join(root, rel)
            text = read(path)
            if eid not in listed:
                problems['page not in docsNav'].append(label)
            fm = FRONTMATTER.match(text)
            if not fm or not re.search(r'^title:\s*\S', fm.group(1), re.M):
                problems['page without title'].append(label)
            check_links(label, path, text)


def check_mirror():
    en, ja = (os.path.join(SITE, lang) for lang in ('en', 'ja'))

    def files(root):
        return {os.path.relpath(os.path.join(d, f), root) for d, _, fs in os.walk(root) for f in fs}

    fe, fj = files(en), files(ja)
    for f in sorted(fe ^ fj):
        problems['not mirrored'].append(f'site: {f} only in {"en" if f in fe else "ja"}')
    for f in sorted(fe & fj):
        if not f.endswith('.md'):
            continue
        he = [(lvl, hid) for lvl, hid, _ in headings(os.path.join(en, f))[1]]
        hj_full = headings(os.path.join(ja, f))[1]
        hj = [(lvl, hid) for lvl, hid, _ in hj_full]
        if he != hj:
            first = next((i for i, (a, b) in enumerate(zip(he, hj)) if a != b), min(len(he), len(hj)))
            problems['headings differ'].append(
                f'site/{f}: en {he[first:first + 1]} vs ja {hj[first:first + 1]}')
        for _, hid, explicit in hj_full:
            if not explicit:
                problems['ja heading without {#id}'].append(f'site/{f}: {hid}')
        ce = split_code(body(read(os.path.join(en, f))))[1]
        cj = split_code(body(read(os.path.join(ja, f))))[1]
        if ce != cj:
            problems['code blocks differ'].append(f'site/{f}: en {len(ce)} blocks, ja {len(cj)}')


def check_dev():
    listed = re.findall(r'\]\(\./([^)]+)\)', read(os.path.join(DEV, 'SUMMARY.md')))
    for page in listed:
        if not os.path.exists(os.path.join(DEV, page)):
            problems['missing page'].append(f'dev: {page}')
    for rel in markdown_files(DEV):
        if rel != 'SUMMARY.md' and rel not in listed:
            problems['page not in SUMMARY'].append(f'dev: {rel}')
        path = os.path.join(DEV, rel)
        check_links(f'dev/{rel}', path, read(path))


check_site()
check_mirror()
check_dev()

for kind, items in problems.items():
    print(f'{kind} ({len(items)})')
    for item in items:
        print(f'  {item}')
if problems:
    sys.exit(1)
print('books ok')
