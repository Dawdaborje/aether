/** One removable chip in the faceted search bar. */
export type SearchOp =
	| 'eq'
	| 'ne'
	| 'like'
	| 'nlike'
	| 'starts'
	| 'ends'
	| 'in'
	| 'nin'
	| 'between'
	| 'null'
	| 'gt'
	| 'gte'
	| 'lt'
	| 'lte'
	/** A whole nested filter from the advanced builder: `value` is an `AdvancedValue`. */
	| 'domain';

/** What the advanced filter builder leaves in a `domain` chip: the kernel filter and the rules it came from. */
export interface AdvancedValue {
	filter: Record<string, unknown>;
	tree: RuleGroup;
}

export interface Rule {
	field: string;
	/** An operator key of the builder (`OP_LABELS` in advanced.ts). */
	op: string;
	/** Text as typed; lists are comma-separated, `between` uses `value` and `value2`. */
	value: string;
	value2?: string;
}

export interface RuleGroup {
	/** `and`: every rule holds. `or`: any rule holds. */
	mode: 'and' | 'or';
	items: (Rule | RuleGroup)[];
}

/** A field the advanced builder can filter on. */
export interface FieldDef {
	field: string;
	label: string;
	/** The column kind (`text`, `integer`, `date`, `boolean`, `selection`, …). */
	type: string;
	options?: FilterOption[];
}

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
	value?: string | string[] | boolean | AdvancedValue | null;
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
