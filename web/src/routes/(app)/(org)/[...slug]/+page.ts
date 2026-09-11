// Org slugs are dynamic and resolved at runtime, so this route cannot be
// prerendered. It is served via the SPA fallback (see svelte.config.js).
export const prerender = false;
