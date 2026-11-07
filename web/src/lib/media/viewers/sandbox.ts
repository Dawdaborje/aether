/**
 * The page a plugin's viewer runs in. It is loaded as `srcdoc` into an `<iframe sandbox="allow-scripts">`
 * (no `allow-same-origin`): the script has an empty origin, no cookies, and cannot call the kernel API or
 * reach the app's page. A content-security policy cuts the network off except for hosts the manifest lists.
 *
 * Protocol (`postMessage`):
 * - frame -> app: `{ t: 'ready' }`, `{ t: 'error', message }`, `{ t: 'action', action: 'download' | 'print' }`
 * - app -> frame: `{ t: 'init', file: { name, mime, size }, code, bytes }` (`bytes` is transferred)
 *
 * The module is an ES module whose default export is called as `default(root, { file, bytes, action })`.
 */
export function sandboxDocument(hosts: string[]): string {
	const connect = hosts.length > 0 ? hosts.join(' ') : "'none'";
	const policy = [
		"default-src 'none'",
		"script-src 'unsafe-inline' blob:",
		"style-src 'unsafe-inline'",
		'img-src data: blob:',
		'media-src data: blob:',
		'font-src data:',
		`connect-src ${connect}`
	].join('; ');
	return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${policy}">
<style>html,body{margin:0;height:100%;font:14px system-ui,sans-serif}#root{min-height:100%}</style>
</head><body><div id="root"></div><script>
const post = (m) => parent.postMessage(m, '*');
addEventListener('message', async (event) => {
  const d = event.data;
  if (!d || d.t !== 'init' || event.source !== parent) return;
  try {
    const url = URL.createObjectURL(new Blob([d.code], { type: 'text/javascript' }));
    const mod = await import(url);
    if (typeof mod.default !== 'function') throw new Error('the viewer module has no default function');
    await mod.default(document.getElementById('root'), {
      file: d.file,
      bytes: d.bytes,
      action: (action) => post({ t: 'action', action }),
    });
  } catch (error) {
    post({ t: 'error', message: String(error && error.message ? error.message : error) });
  }
});
post({ t: 'ready' });
</script></body></html>`;
}
