import { apiFetch } from '$lib/api/client';

export const DEFAULT_LIST_PAGE_SIZE = 10;
export const LIST_PAGE_SIZE_CHOICES = [10, 25, 50, 100] as const;

export type UiPrefs = {
	list_page_size: number;
	list_page_size_choices: number[];
};

let cached: UiPrefs | null = null;
let inflight: Promise<UiPrefs> | null = null;

function normalize(size: unknown): number {
	const n = typeof size === 'number' ? size : typeof size === 'string' ? Number(size) : NaN;
	if (LIST_PAGE_SIZE_CHOICES.includes(n as (typeof LIST_PAGE_SIZE_CHOICES)[number])) return n;
	return DEFAULT_LIST_PAGE_SIZE;
}

/** Default list page size for the current org (global fallback). Cached per session. */
export async function fetchListPageSize(): Promise<number> {
	const prefs = await fetchUiPrefs();
	return prefs.list_page_size;
}

export async function fetchUiPrefs(): Promise<UiPrefs> {
	if (cached) return cached;
	if (inflight) return inflight;
	inflight = (async () => {
		try {
			const res = await apiFetch('/api/ui/prefs');
			if (!res.ok) {
				return {
					list_page_size: DEFAULT_LIST_PAGE_SIZE,
					list_page_size_choices: [...LIST_PAGE_SIZE_CHOICES]
				};
			}
			const body = (await res.json()) as Partial<UiPrefs>;
			const choices = Array.isArray(body.list_page_size_choices)
				? body.list_page_size_choices
						.map((n) => Number(n))
						.filter((n) => LIST_PAGE_SIZE_CHOICES.includes(n as (typeof LIST_PAGE_SIZE_CHOICES)[number]))
				: [...LIST_PAGE_SIZE_CHOICES];
			cached = {
				list_page_size: normalize(body.list_page_size),
				list_page_size_choices: choices.length > 0 ? choices : [...LIST_PAGE_SIZE_CHOICES]
			};
			return cached;
		} catch {
			return {
				list_page_size: DEFAULT_LIST_PAGE_SIZE,
				list_page_size_choices: [...LIST_PAGE_SIZE_CHOICES]
			};
		} finally {
			inflight = null;
		}
	})();
	return inflight;
}

/** Clear cached prefs (e.g. after org switch). */
export function clearUiPrefsCache(): void {
	cached = null;
	inflight = null;
}
