import { apiFetch } from '$lib/api/client';

/** One pack as a registry lists it (`GET /api/ui/emoji`, `GET /api/ui/gifs`). */
export interface ListedPack {
	plugin: string;
	name: string;
	label: string;
	url: string;
}

/**
 * A registry of the message box as the web app sees it: the kernel lists the packs that enabled plugins declare
 * (`[[emoji_pack]]`, `[[gif_pack]]`), and this fetches them. Asked once per page load, however many message boxes
 * there are; a failing pack is left out so the others still work, and says why in the console.
 */
export function packRegistry<T>(kind: string, endpoint: string, valid: (value: unknown) => value is T) {
	let loaded: Promise<ListedPack[]> | null = null;
	const files = new Map<string, Promise<{ pack: T } | null>>();

	function listed(): Promise<ListedPack[]> {
		loaded ??= (async () => {
			try {
				const response = await apiFetch(endpoint);
				if (!response.ok) {
					console.warn(`${kind}: GET ${endpoint} answered ${response.status}; none will be offered`);
					return [];
				}
				const body = (await response.json()) as { packs?: ListedPack[] };
				if (!body.packs?.length) {
					console.warn(
						`${kind}: no enabled plugin has registered a pack for this organization; none will be offered`
					);
				}
				return body.packs ?? [];
			} catch (error) {
				console.warn(`${kind}: could not list the packs`, error);
				return [];
			}
		})();
		return loaded;
	}

	function file(pack: ListedPack): Promise<{ pack: T } | null> {
		let found = files.get(pack.url);
		if (!found) {
			found = apiFetch(pack.url)
				.then((response) => {
					if (!response.ok) console.warn(`${kind}: ${pack.url} answered ${response.status}`);
					return response.ok ? response.json() : null;
				})
				.then((data) => {
					if (data !== null && !valid(data)) console.warn(`${kind}: ${pack.url} is not a ${kind} pack`);
					return valid(data) ? { pack: data } : null;
				})
				.catch((error) => {
					console.warn(`${kind}: could not load ${pack.url}`, error);
					return null;
				});
			files.set(pack.url, found);
		}
		return found;
	}

	return {
		/** The packs that loaded, with who lists them; with `only`, the packs of that name. */
		async load(only?: string): Promise<{ listed: ListedPack; pack: T }[]> {
			const wanted = (await listed()).filter((p) => !only || p.name === only);
			const found = await Promise.all(wanted.map((p) => file(p)));
			const out: { listed: ListedPack; pack: T }[] = [];
			found.forEach((hit, i) => hit && out.push({ listed: wanted[i], pack: hit.pack }));
			return out;
		},
		/** Forget what was loaded, so the next message box asks again (after a plugin is installed or removed). */
		reset(): void {
			loaded = null;
			files.clear();
		}
	};
}
