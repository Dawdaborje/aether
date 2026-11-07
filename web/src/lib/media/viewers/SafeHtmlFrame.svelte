<script lang="ts">
	/**
	 * Shows HTML from a file in a frame that can do nothing: no scripts, no forms, no navigation, no
	 * network (a content-security policy allows only inline styles and images embedded in the file).
	 */
	let { html }: { html: string } = $props();

	const doc = $derived(
		`<!doctype html><html><head><meta charset="utf-8">` +
			`<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:">` +
			`<base target="_blank"><style>body{font:14px system-ui,sans-serif;margin:12px;word-wrap:break-word}img{max-width:100%}</style></head><body>${html}</body></html>`
	);
</script>

<iframe title="Document" sandbox="" srcdoc={doc} class="h-full min-h-64 w-full border-0 bg-white"
></iframe>
