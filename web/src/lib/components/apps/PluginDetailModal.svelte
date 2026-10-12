<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Button } from '$lib/components/ui/button';
	import { categoryLabel } from '$lib/apps/categories';
	import type { CatalogPlugin } from '$lib/apps/catalogApi';
	import { appIcon } from './icons';
	import { orgStore } from '$lib/org/orgStore.svelte';

	let {
		open = $bindable(false),
		plugin,
		onInstall
	}: {
		open?: boolean;
		plugin: CatalogPlugin | null;
		onInstall?: (plugin: CatalogPlugin) => void;
	} = $props();

	const Icon = $derived(plugin ? appIcon(plugin.icon) : undefined);
	let logoFailed = $state(false);

	$effect(() => {
		void plugin?.logo_url;
		logoFailed = false;
	});

	const orgName = (dbName: string) =>
		orgStore.organizations.find((org) => org.db_name === dbName)?.name ?? dbName;
</script>

<Dialog.Root bind:open>
	{#if plugin}
		<Dialog.Content class="max-h-[min(90vh,40rem)] max-w-xl overflow-y-auto">
			<Dialog.Header>
				<div class="flex items-start gap-3">
					<span
						class="flex size-14 shrink-0 items-center justify-center overflow-hidden bg-primary/10 text-primary ring-1 ring-primary/15"
						aria-hidden="true"
					>
						{#if plugin.logo_url && !logoFailed}
							<img
								src={plugin.logo_url}
								alt=""
								class="size-12 object-contain"
								onerror={() => (logoFailed = true)}
							/>
						{:else if Icon}
							<Icon class="size-7" />
						{:else}
							<span class="text-2xl font-semibold">{plugin.label.charAt(0).toUpperCase()}</span>
						{/if}
					</span>
					<div class="min-w-0 space-y-1">
						<Dialog.Title class="text-left">{plugin.label}</Dialog.Title>
						<Dialog.Description class="text-left">
							<code class="text-xs">{plugin.name}</code>
							<span class="text-muted-foreground"> · v{plugin.version}</span>
						</Dialog.Description>
					</div>
				</div>
			</Dialog.Header>

			<div class="space-y-4 text-sm">
				<dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
					<dt class="text-muted-foreground">Category</dt>
					<dd>{categoryLabel(plugin.category)}</dd>
					{#if plugin.kind}
						<dt class="text-muted-foreground">Kind</dt>
						<dd class="capitalize">{plugin.kind}</dd>
					{/if}
					<dt class="text-muted-foreground">Type</dt>
					<dd>
						{plugin.is_app ? 'App' : 'Library'}
						{#if plugin.is_builtin}
							<span class="text-muted-foreground"> · built-in</span>
						{/if}
					</dd>
					{#if plugin.app_route}
						<dt class="text-muted-foreground">Route</dt>
						<dd><code class="text-xs">{plugin.app_route}</code></dd>
					{/if}
					{#if plugin.website}
						<dt class="text-muted-foreground">Website</dt>
						<dd>
							<a
								href={plugin.website}
								class="underline underline-offset-2"
								target="_blank"
								rel="noreferrer">{plugin.website}</a
							>
						</dd>
					{/if}
				</dl>

				{#if plugin.description}
					<p>{plugin.description}</p>
				{/if}
				{#if plugin.app_description && plugin.app_description !== plugin.description}
					<p class="text-muted-foreground">{plugin.app_description}</p>
				{/if}
				{#if plugin.long_description}
					<p class="whitespace-pre-wrap text-muted-foreground">{plugin.long_description}</p>
				{/if}

				{#if plugin.authors.length > 0}
					<section class="space-y-1">
						<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
							Authors
						</h3>
						<ul class="space-y-0.5">
							{#each plugin.authors as author (author.name + (author.email ?? ''))}
								<li>
									{author.name}
									{#if author.email}
										<span class="text-muted-foreground"> · {author.email}</span>
									{/if}
								</li>
							{/each}
						</ul>
					</section>
				{/if}

				{#if plugin.dependencies.length > 0}
					<section class="space-y-1">
						<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
							Depends on
						</h3>
						<p class="flex flex-wrap gap-1.5">
							{#each plugin.dependencies as dep (dep)}
								<code class="rounded-sm bg-muted px-1.5 py-0.5 text-xs">{dep}</code>
							{/each}
						</p>
					</section>
				{/if}

				<section class="space-y-1">
					<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
						Installed in
					</h3>
					{#if plugin.installed_in.length === 0}
						<p class="text-muted-foreground">Not installed in any organization yet.</p>
					{:else}
						<ul class="list-inside list-disc">
							{#each plugin.installed_in as db (db)}
								<li>{orgName(db)}</li>
							{/each}
						</ul>
					{/if}
				</section>
			</div>

			<Dialog.Footer>
				<Button variant="outline" onclick={() => (open = false)}>Close</Button>
				{#if onInstall}
					<Button
						onclick={() => {
							onInstall(plugin);
							open = false;
						}}
					>
						Install into org
					</Button>
				{/if}
			</Dialog.Footer>
		</Dialog.Content>
	{/if}
</Dialog.Root>
