import { apiFetch } from '$lib/api/client';
import { registerViewer, unregisterViewer } from './registry';
import SandboxViewer from './SandboxViewer.svelte';

interface Listed {
	plugin: string;
	name: string;
	label: string;
	mimes: string[];
	extensions: string[];
	priority: number;
	hosts: string[];
	module_url: string;
}

/** Whether a viewer declared with `mimes` / `extensions` opens a file. `image/*` matches a whole family. */
export function declaredMatch(
	mimes: string[],
	extensions: string[],
	file: { name: string; mime: string }
): boolean {
	const mime = (file.mime || '').toLowerCase();
	const ext = file.name.toLowerCase().split('.').pop() ?? '';
	return (
		extensions.includes(ext) ||
		mimes.some((m) => (m.endsWith('/*') ? mime.startsWith(m.slice(0, -1)) : m === mime))
	);
}

let loaded: Promise<void> | null = null;
const ours = new Set<string>();

/** Fetches the plugin viewers of the signed-in organization and registers them, once per page load. */
export function ensurePluginViewers(): Promise<void> {
	loaded ??= (async () => {
		try {
			const response = await apiFetch('/api/ui/viewers');
			if (!response.ok) return;
			const body = (await response.json()) as { viewers?: Listed[] };
			for (const id of ours) unregisterViewer(id);
			ours.clear();
			for (const v of body.viewers ?? []) {
				const id = `plugin:${v.plugin}:${v.name}`;
				ours.add(id);
				registerViewer({
					id,
					label: v.label,
					priority: v.priority,
					matches: (file) => declaredMatch(v.mimes, v.extensions, file),
					component: SandboxViewer,
					spec: { plugin: v.plugin, name: v.name, moduleUrl: v.module_url, hosts: v.hosts }
				});
			}
		} catch {
			/* No plugin viewers: the built-in ones still work. */
		}
	})();
	return loaded;
}

/** Forget what was loaded, so the next file opened asks again (after a plugin is installed or removed). */
export function resetPluginViewers(): void {
	loaded = null;
}
