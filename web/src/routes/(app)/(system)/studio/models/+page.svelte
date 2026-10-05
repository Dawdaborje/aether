<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { authSession } from '$lib/auth/session.svelte';
	import { landingPath } from '$lib/auth/landing';
	import ModelEditor from '$lib/models/ModelEditor.svelte';
	import SaveReport from '$lib/models/SaveReport.svelte';
	import { fetchModels, isValidName, type ModelDef, type PluginModels, type SaveResult } from '$lib/models/api';

	let plugins = $state<PluginModels[] | null>(null);
	let loadError = $state<string | null>(null);
	let selected = $state<{ plugin: string; model: string } | null>(null);
	/** A model made in this page that has not been saved yet. */
	let created = $state<{ plugin: string; model: ModelDef } | null>(null);

	/** What the last save did; kept here because saving re-creates the editor for the new version. */
	let lastSave = $state<SaveResult | null>(null);

	let creating = $state<string | null>(null);
	let newName = $state('');
	let newLabel = $state('');

	onMount(() => {
		if (!authSession.isDeveloper) {
			void goto(resolve(landingPath(authSession.me?.user)));
			return;
		}
		void load();
	});

	async function load() {
		try {
			plugins = await fetchModels();
			loadError = null;
		} catch (error) {
			loadError = error instanceof Error ? error.message : 'Could not load the models';
			plugins = [];
		}
	}

	const plugin = $derived(plugins?.find((p) => p.name === (created?.plugin ?? selected?.plugin)) ?? null);
	const model = $derived(
		created && created.plugin === plugin?.name
			? created.model
			: (plugin?.models.find((m) => m.name === selected?.model) ?? null)
	);
	const modelKey = $derived(`${plugin?.name}/${plugin?.version}/${model?.name}`);

	function select(pluginName: string, modelName: string) {
		created = null;
		lastSave = null;
		selected = { plugin: pluginName, model: modelName };
	}

	function startCreate() {
		if (!creating || !isValidName(newName)) return;
		created = { plugin: creating, model: { name: newName, label: newLabel || undefined, fields: [] } };
		selected = null;
		creating = null;
		newName = newLabel = '';
	}

	async function saved(result: SaveResult) {
		const pluginName = plugin?.name ?? '';
		const modelName = result.model.name;
		lastSave = result;
		created = null;
		await load();
		selected = { plugin: pluginName, model: modelName };
	}
</script>

{#if authSession.isDeveloper}
	<ControlPanel crumbs={[{ label: 'Developer tools', href: '/studio' }, { label: 'Models' }]} />
	<div class="flex w-full flex-1 gap-6 px-4 py-4 md:px-6">
		<aside class="hidden w-64 shrink-0 md:block" aria-label="Plugins and models">
			{#if plugins === null}
				<p class="text-sm text-muted-foreground">Loading…</p>
			{:else if loadError}
				<p class="text-sm text-destructive" role="alert">{loadError}</p>
			{:else}
				<nav class="space-y-4">
					{#each plugins as item (item.name)}
						<div class="space-y-0.5">
							<div class="flex items-center justify-between px-3 pb-1">
								<h2 class="truncate text-xs font-medium text-muted-foreground">{item.label}</h2>
								<button type="button" class="text-muted-foreground hover:text-primary" aria-label="New model in {item.label}" onclick={() => (creating = item.name)}>
									<PlusIcon class="size-3.5" />
								</button>
							</div>
							{#each item.models as entry (entry.name)}
								<button
									type="button"
									onclick={() => select(item.name, entry.name)}
									aria-current={model?.name === entry.name && plugin?.name === item.name ? 'true' : undefined}
									class="flex w-full items-center justify-between rounded-md px-3 py-1.5 text-left text-sm transition hover:bg-accent aria-[current=true]:bg-accent aria-[current=true]:font-medium"
								>
									<span class="truncate">{entry.label ?? entry.name}</span>
									<span class="text-xs text-muted-foreground">{entry.fields.length}</span>
								</button>
							{:else}
								<p class="px-3 text-xs text-muted-foreground">No models.</p>
							{/each}
						</div>
					{/each}
				</nav>
			{/if}
		</aside>

		<main class="min-w-0 flex-1">
			{#if plugin && model}
				{#if lastSave}<SaveReport result={lastSave} ondismiss={() => (lastSave = null)} />{/if}
				{#key modelKey}
					<ModelEditor {plugin} {model} isNew={created !== null} onsaved={saved} />
				{/key}
			{:else}
				<div class="flex flex-col items-center gap-3 rounded-lg border border-dashed border-border px-6 py-16 text-center">
					<DatabaseIcon class="size-8 text-muted-foreground" />
					<p class="font-medium">Choose a model to edit</p>
					<p class="max-w-md text-sm text-muted-foreground">
						A model describes the records of a plugin: its fields, their types and rules. Renaming a field keeps its data, and
						hiding one never deletes it.
					</p>
				</div>
			{/if}
		</main>
	</div>

	<Dialog.Root open={creating !== null} onOpenChange={(open) => !open && (creating = null)}>
		<Dialog.Content class="max-w-md">
			<Dialog.Header>
				<Dialog.Title>New model</Dialog.Title>
				<Dialog.Description>
					The name is used by plugin code, pages and <code>access_models</code>, so choose it carefully: it cannot be changed here
					later. Its label can.
				</Dialog.Description>
			</Dialog.Header>
			<div class="space-y-3">
				<div class="space-y-1.5">
					<label class="text-xs font-medium text-muted-foreground" for="new-model-name">Name</label>
					<Input id="new-model-name" bind:value={newName} class="font-mono" placeholder="invoice" aria-invalid={newName !== '' && !isValidName(newName)} />
				</div>
				<div class="space-y-1.5">
					<label class="text-xs font-medium text-muted-foreground" for="new-model-label">Label</label>
					<Input id="new-model-label" bind:value={newLabel} placeholder="Invoice" />
				</div>
			</div>
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (creating = null)}>Cancel</Button>
				<Button disabled={!isValidName(newName)} onclick={startCreate}>Create</Button>
			</Dialog.Footer>
		</Dialog.Content>
	</Dialog.Root>
{/if}
