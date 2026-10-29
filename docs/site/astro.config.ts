import { unified } from '@astrojs/markdown-remark';
import { defineConfig } from 'astro/config';
import lotus from '@prosefly/astro-theme-lotus';
import kagariGrammar from './grammars/kagari.tmLanguage.json' with { type: 'json' };
import { remarkBookHeadingIds, remarkBookLinks, remarkIncludeSvg } from './src/markdown';

// Site identity, locales, navigation and search live in `theme.config.json`.
export default defineConfig({
  site: 'https://kagari.suitsu31.club',
  markdown: {
    // Lotus adds its own plugins (heading ids, CJK-friendly emphasis) before these.
    processor: unified({
      remarkPlugins: [remarkIncludeSvg, remarkBookLinks, remarkBookHeadingIds],
    }),
  },
  integrations: [
    lotus({
      markdown: {
        // `kis` and `kix` code blocks; the grammar lists `kix` as an alias.
        expressiveCode: { shiki: { langs: [kagariGrammar as never] } },
      },
    }),
  ],
});
