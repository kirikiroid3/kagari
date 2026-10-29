import { defineCollection } from 'astro:content';
import { docsLoader, docsSchema } from '@prosefly/astro-theme-lotus/content';

// The books are plain Markdown (`.md`), not MDX: `{#id}` heading ids and relative `.md`
// links stay readable in editors and on GitHub.
const docs = defineCollection({
  loader: docsLoader({ pattern: '**/*.md' }),
  schema: docsSchema(),
});

export const collections = { docs };
