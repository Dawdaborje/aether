/** One removable chip in the faceted search bar. */
export type SearchOp = 'eq' | 'ne' | 'like' | 'in' | 'null' | 'gt' | 'gte' | 'lt' | 'lte';

export interface SearchChip {
	/** Stable id for list keys. */
	id: string;
	/**
	 * Field name on the model, or `_q` for free-text (expanded by `db::search`
	 * over `search_fields`).
	 */
	field: string;
	op: SearchOp;
	/** Scalar or list for `in`. Ignored for `null`. */
	value?: string | string[] | boolean | null;
	/** Human label shown on the chip. */
	label: string;
}

/** One choice in a `<filter options="…"/>` preset. */
export interface FilterOption {
	/** Value sent in the kernel filter (string or boolean). */
	value: string | boolean;
	/** Label shown in the Filters menu and on chips. */
	label: string;
}

export interface FilterPreset {
	field: string;
	label: string;
	/** Static options for selection filters. */
	options: FilterOption[];
}

export interface GroupByOption {
	field: string;
	label: string;
}

/** Payload slice sent to a list `source` function. */
export interface ListSearchPayload {
	filter: Record<string, unknown>;
	q?: string;
	search_fields?: string[];
	group_by?: string[];
}
