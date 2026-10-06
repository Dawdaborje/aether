import { apiFetch } from '$lib/api/client';

export type FieldType =
	| 'string'
	| 'text'
	| 'int'
	| 'float'
	| 'decimal'
	| 'bool'
	| 'date'
	| 'datetime'
	| 'select'
	| 'link'
	| 'many2many'
	| 'json';

export const FIELD_TYPES: { value: FieldType; label: string }[] = [
	{ value: 'string', label: 'Text (short)' },
	{ value: 'text', label: 'Text (long)' },
	{ value: 'int', label: 'Whole number' },
	{ value: 'float', label: 'Number' },
	{ value: 'decimal', label: 'Exact decimal (money)' },
	{ value: 'bool', label: 'Yes / no' },
	{ value: 'date', label: 'Date' },
	{ value: 'datetime', label: 'Date and time' },
	{ value: 'select', label: 'Choice' },
	{ value: 'link', label: 'Link to a record' },
	{ value: 'many2many', label: 'Links to several records' },
	{ value: 'json', label: 'JSON' }
];

export interface SelectOption {
	id?: string;
	value: string;
	label?: string;
	color?: string;
}

export interface FieldDef {
	/** Assigned by the server for a new field; never changes after. */
	id?: string;
	name: string;
	label?: string;
	type: FieldType;
	required?: boolean;
	default?: unknown;
	max_length?: number;
	index?: 'plain' | 'unique';
	options?: SelectOption[];
	target?: string;
	help?: string;
	/** Hidden: plugins no longer see it; the data is kept. */
	deprecated?: boolean;
	/** Changes to this field are written in the record's chatter. */
	track?: boolean;
	/** Properties the editor has no control for are kept as they are when a model is saved. */
	scale?: number;
	hierarchy?: boolean;
	target_id?: string;
	min?: unknown;
	max?: unknown;
	min_length?: number;
	pattern?: string;
	sequence?: { pattern: string; reset?: 'never' | 'yearly' | 'monthly' };
	related?: string;
	compute?: string;
}

export type VisitorChatter = 'none' | 'read' | 'read_write';

/** A model's chatter: the conversation and history of its records. Off unless `enabled`. */
export interface ChatterDef {
	enabled?: boolean;
	messages?: boolean;
	notes?: boolean;
	followers?: boolean;
	track_changes?: boolean;
	visitors?: VisitorChatter;
}

export interface ModelDef {
	model_id?: string;
	name: string;
	label?: string;
	plural_label?: string;
	icon?: string;
	title_field?: string;
	chatter?: ChatterDef;
	fields: FieldDef[];
	/** How the model is shown; kept as it is. */
	view?: unknown;
	/** Indexes over several fields, and checks on the whole record; kept as they are. */
	indexes?: unknown[];
	checks?: unknown[];
}

export interface PluginModels {
	name: string;
	label: string;
	version: string;
	models: ModelDef[];
	/** The plugin's source folder is on this machine, so the model file can be written. */
	file_available: boolean;
}

export interface OrgPlan {
	organization: string;
	changes: { kind: string; field?: string; type?: string; unique?: boolean }[];
	blockers: string[];
	notes: string[];
}

export interface SaveResult {
	version: string;
	from: string;
	model: ModelDef;
	file: { written: boolean; path?: string; reason?: string };
	applied: { organization: string; ok: boolean; error?: string }[];
	note: string;
}

async function failure(res: Response, fallback: string): Promise<Error> {
	const body = await res.json().catch(() => ({}));
	return new Error(typeof body.error === 'string' ? body.error : `${fallback} (${res.status})`);
}

const json = { 'Content-Type': 'application/json' };

export async function fetchModels(): Promise<PluginModels[]> {
	const res = await apiFetch('/api/ui/models');
	if (!res.ok) throw await failure(res, 'Could not load the models');
	return ((await res.json()) as { plugins: PluginModels[] }).plugins;
}

/** What saving `model` would do in each of `organizations`. */
export async function planModel(
	plugin: string,
	model: ModelDef,
	organizations: string[]
): Promise<OrgPlan[]> {
	const res = await apiFetch(`/api/ui/models/${encodeURIComponent(plugin)}/plan`, {
		method: 'POST',
		headers: json,
		body: JSON.stringify({ model, organizations })
	});
	if (!res.ok) throw await failure(res, 'Could not preview the change');
	return ((await res.json()) as { organizations: OrgPlan[] }).organizations;
}

export async function saveModel(
	plugin: string,
	model: ModelDef,
	options: { writeFile: boolean; applyTo: string[] }
): Promise<SaveResult> {
	const res = await apiFetch(`/api/ui/models/${encodeURIComponent(plugin)}`, {
		method: 'PUT',
		headers: json,
		body: JSON.stringify({
			model,
			write_file: options.writeFile,
			apply_to: options.applyTo
		})
	});
	if (!res.ok) throw await failure(res, 'Could not save the model');
	return (await res.json()) as SaveResult;
}

/** The field names a model's draft may use: lowercase letters, digits and `_`, starting with a letter. */
export function isValidName(name: string): boolean {
	return /^[a-z][a-z0-9_]{0,47}$/.test(name);
}
