import type { FieldType } from './types';

/** What the server adds to a page field from its model (`plugin_manager/field_hints.rs`). */
export interface FieldHintNode {
	fieldType?: unknown;
	type?: unknown;
	source?: unknown;
	options?: unknown;
	label?: unknown;
	name?: unknown;
	field?: unknown;
	help?: unknown;
	required?: unknown;
	modelType?: unknown;
	modelLabel?: unknown;
	modelHelp?: unknown;
	modelRequired?: unknown;
	modelScale?: unknown;
	modelOptions?: unknown;
	modelTarget?: unknown;
	modelMultiple?: unknown;
}

export interface FieldOption {
	value: string;
	label: string;
	/** A colour name from the model's option (`gray`, `green`, …), for badges. */
	color?: string;
}

export interface ResolvedField {
	/** The widget to draw. The author's `fieldType` wins; otherwise the model's type. */
	fieldType: FieldType;
	/** Where `fieldType` came from: written on the page, or inferred from the model. */
	inferred: boolean;
	label: string;
	help: string;
	required: boolean;
	options: FieldOption[];
	/** Digits after the point of a `decimal`. */
	scale: number | null;
	/** The model a link points at (`party.party`), when the model says. */
	target: string | null;
	multiple: boolean;
}

/** Model type (`FieldType` in `data_model/definition.rs`) to the widget that edits it. */
const FROM_MODEL: Record<string, FieldType> = {
	string: 'char',
	text: 'text',
	int: 'integer',
	float: 'float',
	decimal: 'decimal',
	bool: 'boolean',
	date: 'date',
	datetime: 'datetime',
	select: 'selection',
	link: 'link',
	many2many: 'many2many',
	json: 'json',
	image: 'image',
	file: 'file'
};

/**
 * Author-written widget names that are older spellings of the same thing, so a page
 * saying `fieldType="number"` or `"bool"` gets the widget it meant.
 */
const ALIASES: Record<string, FieldType> = {
	number: 'float',
	int: 'integer',
	bool: 'boolean',
	string: 'char',
	select: 'selection',
	switch: 'boolean'
};

function str(value: unknown): string | null {
	return typeof value === 'string' && value.trim() ? value.trim() : null;
}

/** `options="a, b"` (a list of values) or the model's `[{ value, label?, color? }]`. */
export function resolveOptions(node: FieldHintNode): FieldOption[] {
	const written = node.options;
	if (Array.isArray(written) && written.length > 0) {
		return written.map((option) => ({ value: String(option), label: String(option) }));
	}
	if (typeof written === 'string' && written.trim()) {
		return written
			.split(',')
			.map((option) => option.trim())
			.filter(Boolean)
			.map((option) => ({ value: option, label: option }));
	}
	const modeled = node.modelOptions;
	if (!Array.isArray(modeled)) return [];
	return modeled.flatMap((option): FieldOption[] => {
		if (typeof option !== 'object' || option === null) return [];
		const { value, label, color } = option as Record<string, unknown>;
		if (typeof value !== 'string') return [];
		return [
			{
				value,
				label: str(label) ?? humanize(value),
				color: str(color) ?? undefined
			}
		];
	});
}

/** `on_leave` → `On leave`. */
export function humanize(value: string): string {
	const spaced = value.replaceAll('_', ' ').trim();
	return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

/**
 * Decide how a page field is drawn, from what the author wrote and what the model says.
 *
 * Order: the page's `fieldType` (even an old spelling of one), then, when it names a
 * `source` function and no type, a link picker; then the model's own type; then text.
 */
export function resolveField(node: FieldHintNode): ResolvedField {
	const written = str(node.fieldType);
	const modelType = str(node.modelType);
	const source = str(node.source);

	let fieldType: FieldType;
	let inferred = false;
	if (written) {
		fieldType = ALIASES[written] ?? (written as FieldType);
	} else if (source && (modelType === null || modelType === 'string' || modelType === 'link')) {
		// A `source` function is a list of rows to pick from.
		fieldType = 'link';
	} else if (modelType && FROM_MODEL[modelType]) {
		fieldType = FROM_MODEL[modelType];
		inferred = true;
	} else {
		fieldType = 'char';
		inferred = true;
	}

	const name = str(node.name) ?? str(node.field) ?? 'Field';
	const scale = typeof node.modelScale === 'number' ? node.modelScale : null;
	return {
		fieldType,
		inferred,
		label: str(node.label) ?? str(node.modelLabel) ?? name,
		help: str(node.help) ?? str(node.modelHelp) ?? '',
		required: node.required === true || node.modelRequired === true,
		options: resolveOptions(node),
		scale: fieldType === 'decimal' ? (scale ?? 2) : scale,
		target: str(node.modelTarget),
		multiple: node.modelMultiple === true
	};
}

/**
 * The text a number input steps by: one unit of the last digit of a decimal, `1` for an
 * integer, `any` for a float.
 */
export function numberStep(field: Pick<ResolvedField, 'fieldType' | 'scale'>): string | undefined {
	if (field.fieldType === 'integer') return '1';
	if (field.fieldType === 'decimal') {
		const scale = field.scale ?? 2;
		return scale <= 0 ? '1' : `0.${'0'.repeat(scale - 1)}1`;
	}
	if (field.fieldType === 'float' || field.fieldType === 'currency') return 'any';
	return undefined;
}
