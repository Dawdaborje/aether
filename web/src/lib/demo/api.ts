import { apiFetch } from '$lib/api/client';

export interface DemoModelEntry {
	name: string;
	label: string;
	command: string;
}

export interface DemoPluginEntry {
	name: string;
	label: string;
	version: string;
	models: DemoModelEntry[];
}

export interface GenerateDemoResult {
	ok: boolean;
	plugin: string;
	command: string;
	model: string;
	count: number;
	result: unknown;
}

async function readError(res: Response, fallback: string): Promise<string> {
	const body = (await res.json().catch(() => ({}))) as { error?: string };
	return typeof body.error === 'string' ? body.error : fallback;
}

/** Plugins installed in `org` that declare at least one `demo_model` command. */
export async function fetchDemoCatalog(org: string): Promise<DemoPluginEntry[]> {
	const res = await apiFetch(`/api/ui/demo?org=${encodeURIComponent(org)}`);
	if (!res.ok) {
		throw new Error(await readError(res, `Demo catalog could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { plugins: DemoPluginEntry[] };
	return body.plugins ?? [];
}

/** Run the plugin's demo generator for one model. */
export async function generateDemo(input: {
	org: string;
	plugin: string;
	model: string;
	count: number;
}): Promise<GenerateDemoResult> {
	const res = await apiFetch('/api/ui/demo/generate', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(input)
	});
	if (!res.ok) {
		throw new Error(await readError(res, `Demo generation failed (${res.status}).`));
	}
	return (await res.json()) as GenerateDemoResult;
}
