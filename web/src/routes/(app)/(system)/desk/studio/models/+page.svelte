<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { onMount } from 'svelte';
	import DatabaseIcon from '@lucide/svelte/icons/database';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SearchIcon from '@lucide/svelte/icons/search';
	import BoxesIcon from '@lucide/svelte/icons/boxes';
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { SearchPicker } from '$lib/components/ui/search-picker';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { authSession } from '$lib/auth/session.svelte';
	import { landingPath } from '$lib/auth/landing';
	import ModelEditor from '$lib/models/ModelEditor.svelte';
	import SaveReport from '$lib/models/SaveReport.svelte';
	import {
		fetchModels,
		isValidName,
		modelListLabel,
		type ModelDef,
		type PluginModels,
		type SaveResult
	} from '$lib/models/api';

	let plugins = $state<PluginModels[] | null>(null);
	let loadError = $state<string | null>(null);
	/** Which plugin's models are listed in the side panel. */
	let focusPlugin = $state('');
	let selected = $state<{ plugin: string; model: string } | null>(null);
	/** A model made in this page that has not been saved yet. */
	let created = $state<{ plugin: string; model: ModelDef } | null>(null);

	/** What the last save did; kept here because saving re-creates the editor for the new version. */
	let lastSave = $state<SaveResult | null>(null);

	let creating = $state(false);
	let createPlugin = $state('');
	let newName = $state('');
	let newLabel = $state('');
	let query = $state('');

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
			if (!focusPlugin && plugins.length) {
				focusPlugin = selected?.plugin ?? created?.plugin ?? plugins[0]!.name;
			} else if (focusPlugin && !plugins.some((p) => p.name === focusPlugin)) {
				focusPlugin = plugins[0]?.name ?? '';
			}
		} catch (error) {
			loadError = error instanceof Error ? error.message : 'Could not load the models';
			plugins = [];
		}
	}

	const activePlugin = $derived(plugins?.find((p) => p.name === focusPlugin) ?? null);
	const editorPlugin = $derived(
		plugins?.find((p) => p.name === (created?.plugin ?? selected?.plugin)) ?? null
	);
	const model = $derived(
		created && created.plugin === editorPlugin?.name
			? created.model
			: (editorPlugin?.models.find((m) => m.name === selected?.model) ?? null)
	);
	const modelKey = $derived(`${editorPlugin?.name}/${editorPlugin?.version}/${model?.name}`);

	const listedModels = $derived.by(() => {
		const models = activePlugin?.models ?? [];
		const q = query.trim().toLowerCase();
		if (!q) return models;
		return models.filter((entry) => {
			const label = modelListLabel(entry).toLowerCase();
			return label.includes(q) || entry.name.toLowerCase().includes(q);
		});
	});

	const pluginPickerItems = $derived(
		(plugins ?? []).map((item) => ({
			id: item.name,
			label: item.label,
			sublabel: `${item.name} · v${item.version}`
		}))
	);

	function onPluginChange(name: string) {
		focusPlugin = name;
		query = '';
		// Keep the editor if it already belongs to this plugin; otherwise clear selection.
		if (selected && selected.plugin !== name) {
			selected = null;
			created = null;
			lastSave = null;
		}
	}

	function select(pluginName: string, modelName: string) {
		created = null;
		lastSave = null;
		selected = { plugin: pluginName, model: modelName };
		focusPlugin = pluginName;
	}

	function openCreate(pluginName?: string) {
		createPlugin = pluginName ?? focusPlugin ?? plugins?.[0]?.name ?? '';
		newName = '';
		newLabel = '';
		creating = true;
	}

	function startCreate() {
		if (!createPlugin || !isValidName(newName)) return;
		created = {
			plugin: createPlugin,
			model: { name: newName, label: newLabel || undefined, fields: [] }
		};
		selected = null;
		focusPlugin = createPlugin;
		creating = false;
		newName = newLabel = '';
	}

	async function saved(result: SaveResult) {
		const pluginName = editorPlugin?.name ?? focusPlugin;
		const modelName = result.model.name;
		lastSave = result;
		created = null;
		await load();
		selected = { plugin: pluginName, model: modelName };
		focusPlugin = pluginName;
	}
</script>

{#if authSession.isDeveloper}
	<div class="flex h-full min-h-0 w-full flex-col">
	<ControlPanel crumbs={[{ label: 'Developer tools', href: '/desk/studio' }, { label: 'Models' }]}>
		{#snippet actions()}
			<Button size="sm" onclick={() => openCreate()}>
				<PlusIcon class="size-3.5" />
				New model
			</Button>
		{/snippet}
	</ControlPanel>
	<div class="flex min-h-0 flex-1 overflow-hidden">
		<!-- Stays put while the field editor scrolls in the right pane. -->
		<aside
			class="flex w-72 shrink-0 flex-col border-r border-border bg-card"
			aria-label="Plugin models"
		>
			<div class="space-y-3 border-b border-border p-3">
				{#if plugins === null}
					<p class="text-sm text-muted-foreground">Loading…</p>
				{:else if loadError}
					<p class="text-sm text-destructive" role="alert">{loadError}</p>
				{:else}
					<SearchPicker
						items={pluginPickerItems}
						value={focusPlugin || null}
						preferId={selected?.plugin ?? created?.plugin ?? null}
						label="Plugin"
						placeholder="Select a plugin…"
						searchPlaceholder="Search plugins…"
						entityLabel="plugins"
						icon={BoxesIcon}
						onchange={onPluginChange}
					/>
					<div class="relative">
						<SearchIcon
							class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
						/>
						<Input
							bind:value={query}
							type="search"
							placeholder="Search this plugin…"
							aria-label="Search models in plugin"
							class="h-9 pl-8"
						/>
					</div>
					<div class="flex items-center justify-between gap-2">
						<p class="truncate text-xs text-muted-foreground">
							{listedModels.length} model{listedModels.length === 1 ? '' : 's'}
							{#if activePlugin}
								· v{activePlugin.version}
							{/if}
						</p>
						<button
							type="button"
							class="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-primary"
							aria-label="New model in selected plugin"
							disabled={!focusPlugin}
							onclick={() => openCreate(focusPlugin)}
						>
							<PlusIcon class="size-3.5" />
						</button>
					</div>
				{/if}
			</div>
			<nav class="min-h-0 flex-1 overflow-y-auto p-1">
				{#if activePlugin}
					<ul class="space-y-0.5">
						{#each listedModels as entry (entry.name)}
							<li>
								<button
									type="button"
									onclick={() => select(activePlugin.name, entry.name)}
									aria-current={model?.name === entry.name &&
									editorPlugin?.name === activePlugin.name
										? 'true'
										: undefined}
									class="flex w-full items-center justify-between gap-2 rounded-md px-2.5 py-2 text-left text-sm transition hover:bg-accent aria-[current=true]:bg-accent aria-[current=true]:font-medium"
								>
									<span class="min-w-0 truncate">
										<span class="block truncate">{modelListLabel(entry)}</span>
										<span class="block truncate font-mono text-xs text-muted-foreground"
											>{entry.name}</span
										>
									</span>
									<span class="shrink-0 text-xs text-muted-foreground">{entry.fields.length}</span>
								</button>
							</li>
						{:else}
							<li class="px-2.5 py-3 text-xs text-muted-foreground">
								{query.trim() ? 'No matching models.' : 'No models in this plugin.'}
							</li>
						{/each}
					</ul>
				{/if}
			</nav>
		</aside>

		<main class="min-w-0 flex-1 overflow-y-auto px-4 py-4 md:px-6">
			{#if editorPlugin && model}
				{#if lastSave}<SaveReport result={lastSave} ondismiss={() => (lastSave = null)} />{/if}
				{#key modelKey}
					<ModelEditor
						plugin={editorPlugin}
						{model}
						catalog={plugins ?? []}
						isNew={created !== null}
						onsaved={saved}
					/>
				{/key}
			{:else}
				<div
					class="flex flex-col items-center gap-3 rounded-lg border border-dashed border-border px-6 py-16 text-center"
				>
					<DatabaseIcon class="size-8 text-muted-foreground" />
					<p class="font-medium">Choose a model to edit</p>
					<p class="max-w-md text-sm text-muted-foreground">
						Select a plugin above, then pick one of its models. Creating a model also starts with
						choosing the plugin that will own it.
					</p>
					<Button size="sm" onclick={() => openCreate()} disabled={!focusPlugin}>
						<PlusIcon class="size-3.5" />
						New model
					</Button>
				</div>
			{/if}
		</main>
	</div>
	</div>

	<Dialog.Root bind:open={creating}>
		<Dialog.Content class="max-w-md">
			<Dialog.Header>
				<Dialog.Title>New model</Dialog.Title>
				<Dialog.Description>
					Pick the plugin that will own this model, then choose a technical name. The name cannot be
					changed later; its label can.
				</Dialog.Description>
			</Dialog.Header>
			<div class="space-y-3">
				<SearchPicker
					items={pluginPickerItems}
					value={createPlugin || null}
					preferId={focusPlugin || null}
					label="Plugin"
					placeholder="Select a plugin…"
					searchPlaceholder="Search plugins…"
					entityLabel="plugins"
					icon={BoxesIcon}
					onchange={(name) => (createPlugin = name)}
				/>
				<div class="space-y-1.5">
					<Label for="new-model-name">Name</Label>
					<Input
						id="new-model-name"
						bind:value={newName}
						class="font-mono"
						placeholder="invoice"
						aria-invalid={newName !== '' && !isValidName(newName)}
					/>
				</div>
				<div class="space-y-1.5">
					<Label for="new-model-label">Label</Label>
					<Input id="new-model-label" bind:value={newLabel} placeholder="Invoice" />
					<p class="text-xs text-muted-foreground">
						Optional. Use plain text, or a translation key like <code>%invoice%</code>.
					</p>
				</div>
			</div>
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (creating = false)}>Cancel</Button>
				<Button disabled={!createPlugin || !isValidName(newName)} onclick={startCreate}
					>Create</Button
				>
			</Dialog.Footer>
		</Dialog.Content>
	</Dialog.Root>
{/if}
