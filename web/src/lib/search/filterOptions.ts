import type { FilterOption } from './types';

/**
 * Parse `<filter options="…"/>`.
 *
 * - `active,leave` → value and label are the same token
 * - `Active=true,Inactive=false` → label before `=`, value after (booleans coerced)
 */
export function parseFilterOptions(raw: string | null | undefined): FilterOption[] {
	return (raw ?? '')
		.split(',')
		.map((part) => part.trim())
		.filter(Boolean)
		.map((part) => {
			const eq = part.indexOf('=');
			if (eq <= 0) return { value: part, label: part };
			const label = part.slice(0, eq).trim();
			const rawValue = part.slice(eq + 1).trim();
			const value =
				rawValue === 'true' ? true : rawValue === 'false' ? false : rawValue;
			return { value, label: label || String(value) };
		});
}
