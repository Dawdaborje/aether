import { replaceState } from '$app/navigation';
import { page } from '$app/state';
import { withParams } from './filterParams';

/**
 * Keep a page's filters in the address bar. Call it once while the component is being set
 * up, with a function that returns the current filter values. A value equal to its default
 * (or empty) is left out, and the history entry is replaced so Back is not flooded.
 * Read the starting values with `textParam`, `choiceParam` and `numberParam` from
 * `page.url.searchParams`.
 */
export function syncFilters(
	values: () => Record<string, string | number | null | undefined>,
	defaults: Record<string, string | number> = {}
) {
	$effect(() => {
		const next = withParams(page.url, values(), defaults);
		if (next === `${page.url.pathname}${page.url.search}${page.url.hash}`) return;
		try {
			// eslint-disable-next-line svelte/no-navigation-without-resolve -- same page, only the query changes
			replaceState(next, page.state);
		} catch {
			// The router is not ready yet (first paint); the next change writes it.
		}
	});
}
