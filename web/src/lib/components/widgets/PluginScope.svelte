<script lang="ts">
	import type { Snippet } from 'svelte';
	import { PageState, getPageState, setPageState } from '$lib/pages/pageContext.svelte';

	/**
	 * A tab another plugin added to this page (`<page extends="…">`): its widgets call
	 * `plugin`'s functions, and see the page's route values renamed by `params`
	 * (`employee={id}` hands that plugin the employee id as `employee`).
	 */
	let {
		plugin,
		params = '',
		children
	}: { plugin: string; params?: string; children: Snippet } = $props();

	const parent = getPageState();
	const scoped = new PageState();
	scoped.refresh = () => parent?.refresh();
	setPageState(scoped);

	function mapParams(spec: string, from: Record<string, string>): Record<string, string> {
		const out: Record<string, string> = { ...from };
		for (const part of spec.split(',')) {
			const eq = part.indexOf('=');
			if (eq < 1) continue;
			const key = part.slice(0, eq).trim();
			const raw = part.slice(eq + 1).trim();
			const ref = raw.match(/^\{(\w+)\}$/);
			const value = ref ? from[ref[1]] : raw;
			// A route value the page does not have (`/hr/me` has no `{id}`) is left out, so the
			// plugin applies its own default (the caller).
			if (value) out[key] = value;
			else delete out[key];
		}
		return out;
	}

	$effect.pre(() => {
		scoped.plugin = plugin;
		scoped.model = '';
		scoped.params = mapParams(params, parent?.params ?? {});
		scoped.tick = parent?.tick ?? 0;
	});
</script>

{@render children()}
