// Remark plugins that keep the book sources plain, editor- and GitHub-friendly Markdown:
// links between pages are relative `.md` paths, headings get mdBook-style ids (the same
// algorithm as `docs/check-books.py`), and diagrams are SVG files included inline.
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

interface Node {
  type: string;
  value?: string;
  url?: string;
  depth?: number;
  children?: Node[];
  position?: { start: { offset?: number }; end: { offset?: number } };
  data?: { hProperties?: Record<string, unknown> };
}

interface File {
  path?: string;
  value: unknown;
}

/** The content collection root: `src/content/docs/<locale directory>/...`. */
const CONTENT_DIR = fileURLToPath(new URL('./content/docs/', import.meta.url));

/** Locale directory -> URL prefix. Japanese is the root locale. */
const LOCALE_PREFIX: Record<string, string> = { ja: '', en: '/en' };

function walk(node: Node, visit: (node: Node, parent: Node | undefined, index: number) => void) {
  const go = (n: Node, parent: Node | undefined, index: number) => {
    visit(n, parent, index);
    n.children?.forEach((child, i) => go(child, n, i));
  };
  go(node, undefined, -1);
}

function sourcePath(file: File): string {
  if (!file.path) {
    throw new Error('Kagari markdown plugins need the source file path');
  }
  return file.path;
}

/** mdBook's heading id: link text kept, inline code unwrapped, HTML dropped, then lowercase
 * letters, digits, `_` and `-` kept and spaces turned into `-`. */
export function slug(text: string): string {
  const unlinked = text.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1');
  const plain = unlinked
    .split(/(`[^`]*`)/)
    .map((part) => (part.startsWith('`') ? part.slice(1, -1) : part.replace(/<[^>]+>/g, '')))
    .join('');
  return [...plain.trim().toLowerCase()]
    .map((c) => (/[\p{L}\p{N}_-]/u.test(c) ? c : c === ' ' ? '-' : ''))
    .join('');
}

/** Gives every heading the id mdBook (and `check-books.py`) computes: the explicit `{#id}` if
 * present, else the slug of its source text, `-1`, `-2`, … for repeats. Runs after Lotus's own
 * heading-id plugin, whose slugs drop inline code, and overrides them. */
export function remarkBookHeadingIds() {
  return (tree: Node, file: File) => {
    const source = String(file.value);
    const counts = new Map<string, number>();
    walk(tree, (node) => {
      const start = node.position?.start.offset;
      const end = node.position?.end.offset;
      if (node.type !== 'heading' || start === undefined || end === undefined) {
        return;
      }
      const raw = source.slice(start, end).replace(/^#{1,6}\s+/, '').trim();
      const explicit = raw.match(/\{#([^}\s]+)[^}]*\}\s*$/);
      let id: string;
      if (explicit) {
        id = explicit[1];
      } else {
        const base = slug(raw);
        const seen = counts.get(base) ?? 0;
        id = seen ? `${base}-${seen}` : base;
        counts.set(base, seen + 1);
      }
      node.data = { ...node.data, hProperties: { ...node.data?.hProperties, id } };
    });
  };
}

/** Rewrites relative links to `.md` pages into site routes:
 * `ja/reference/syntax/forms.md#x` -> `/reference/syntax/forms/#x`,
 * `en/std/index.md` -> `/en/std/`. A link to a missing page fails the build. */
export function remarkBookLinks() {
  return (tree: Node, file: File) => {
    const from = sourcePath(file);
    walk(tree, (node) => {
      if ((node.type !== 'link' && node.type !== 'definition') || !node.url) {
        return;
      }
      const [target, hash] = node.url.split('#', 2);
      if (!target.endsWith('.md') || /^[a-z][a-z0-9+.-]*:/i.test(target) || target.startsWith('/')) {
        return;
      }
      const abs = path.resolve(path.dirname(from), decodeURI(target));
      const rel = path.relative(CONTENT_DIR, abs).split(path.sep);
      const prefix = LOCALE_PREFIX[rel[0]];
      if (!existsSync(abs) || prefix === undefined) {
        throw new Error(`${from}: link to a missing page: ${node.url}`);
      }
      const id = rel.slice(1).join('/').replace(/\.md$/, '').replace(/(^|\/)index$/, '');
      node.url = `${prefix}/${id ? `${id}/` : ''}${hash ? `#${hash}` : ''}`;
    });
  };
}

/** Inlines `{{#include path/to/diagram.svg}}` paragraphs, so the diagram's `var(--fg)` colors
 * follow the page theme (see `.kg-diagram` in `styles/site.css`). */
export function remarkIncludeSvg() {
  return (tree: Node, file: File) => {
    const from = sourcePath(file);
    walk(tree, (node, parent, index) => {
      // Lotus's `{#id}` syntax extension splits `{#…}` into its own text node: join the
      // paragraph's text children before matching.
      const children = node.type === 'paragraph' ? (node.children ?? []) : [];
      const text = children.every((c) => c.type === 'text') ? children.map((c) => c.value).join('') : '';
      const m = text.trim().match(/^\{\{#include ([^}]+\.svg)\}\}$/);
      if (!m || !parent?.children) {
        return;
      }
      const svgPath = path.resolve(path.dirname(from), m[1].trim());
      if (!existsSync(svgPath)) {
        throw new Error(`${from}: missing diagram ${m[1]}`);
      }
      const svg = readFileSync(svgPath, 'utf8').replace(/^<\?xml[^>]*>\s*/, '');
      parent.children[index] = { type: 'html', value: `<figure class="kg-diagram">${svg}</figure>` };
    });
  };
}
