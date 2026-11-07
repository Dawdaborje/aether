/**
 * Keep a page's filters in the address bar, so a refresh or a shared link restores them.
 * These are plain helpers; the page decides which filters it has and when to write them.
 */

/** A text filter from the query string, or `fallback` when it is missing. */
export function textParam(params: URLSearchParams, key: string, fallback = ''): string {
	return params.get(key) ?? fallback;
}

/** A filter that must be one of `allowed`; anything else (a stale or edited link) gives `fallback`. */
export function choiceParam<T extends string>(
	params: URLSearchParams,
	key: string,
	allowed: readonly T[],
	fallback: T
): T {
	const value = params.get(key);
	return value !== null && (allowed as readonly string[]).includes(value) ? (value as T) : fallback;
}

/** A numeric filter that must be one of `allowed`. */
export function numberParam<T extends number>(
	params: URLSearchParams,
	key: string,
	allowed: readonly T[],
	fallback: T
): T {
	const value = Number(params.get(key));
	return params.has(key) && (allowed as readonly number[]).includes(value)
		? (value as T)
		: fallback;
}

/**
 * The address for `url` with `values` written into its query string. A value equal to its
 * default (or empty) is removed, so an untouched page keeps a clean address. Other parameters
 * already in the address (like `?org=`) and the hash are kept.
 */
export function withParams(
	url: URL,
	values: Record<string, string | number | null | undefined>,
	defaults: Record<string, string | number> = {}
): string {
	const params = new URLSearchParams(url.search);
	for (const [key, value] of Object.entries(values)) {
		const text = value === null || value === undefined ? '' : String(value).trim();
		if (text === '' || text === String(defaults[key] ?? '')) params.delete(key);
		else params.set(key, text);
	}
	const search = params.toString();
	return `${url.pathname}${search ? `?${search}` : ''}${url.hash}`;
}
