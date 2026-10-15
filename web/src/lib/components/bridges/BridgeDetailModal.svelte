<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { categoryLabel } from '$lib/apps/categories';
	import { setBridgeEnabled, type Bridge } from '$lib/bridges/api';
	import { fetchSettingsCatalog, type CatalogGroup } from '$lib/settings/api';
	import BridgeLogo from './BridgeLogo.svelte';

	let {
		open = $bindable(false),
		bridge = $bindable(null),
		onUpdated
	}: {
		open?: boolean;
		bridge?: Bridge | null;
		/** Called after enable/disable so the list can refresh that row. */
		onUpdated?: (bridge: Bridge) => void;
	} = $props();

	let working = $state(false);
	let error = $state<string | null>(null);
	let settingsGroup = $state<CatalogGroup | null>(null);
	let settingsLoaded = $state(false);

	$effect(() => {
		if (!open || !bridge) {
			settingsGroup = null;
			settingsLoaded = false;
			error = null;
			return;
		}
		const key = bridge.feature_key;
		let cancelled = false;
		settingsLoaded = false;
		settingsGroup = null;
		void fetchSettingsCatalog()
			.then((catalog) => {
				if (cancelled) return;
				const prefix = `bridge.${key}.`;
				settingsGroup =
					catalog.groups.find((group) => group.items.some((item) => item.key.startsWith(prefix))) ??
					null;
				settingsLoaded = true;
			})
			.catch(() => {
				if (cancelled) return;
				settingsGroup = null;
				settingsLoaded = true;
			});
		return () => {
			cancelled = true;
		};
	});

	async function setEnabled(enabled: boolean) {
		if (!bridge) return;
		working = true;
		error = null;
		try {
			const updated = await setBridgeEnabled(bridge.feature_key, enabled);
			bridge = updated;
			onUpdated?.(updated);
		} catch (err) {
			error = err instanceof Error ? err.message : 'Could not update the bridge';
		} finally {
			working = false;
		}
	}

	async function openSettings() {
		if (!settingsGroup) return;
		open = false;
		await goto(resolve(`/desk/settings/${settingsGroup.slug}`));
	}
</script>

<Dialog.Root bind:open>
	{#if bridge}
		<Dialog.Content class="max-h-[min(90vh,36rem)] max-w-lg overflow-y-auto sm:max-w-lg">
			<Dialog.Header>
				<div class="flex items-start gap-3 pr-8">
					<BridgeLogo featureKey={bridge.feature_key} label={bridge.label} size="lg" />
					<div class="min-w-0 space-y-1.5">
						<div class="flex flex-wrap items-center gap-2">
							<Dialog.Title class="text-left">{bridge.label}</Dialog.Title>
							<Badge variant={bridge.enabled_globally ? 'default' : 'secondary'}>
								{bridge.enabled_globally ? 'Enabled' : 'Not enabled'}
							</Badge>
						</div>
						<Dialog.Description class="text-left">
							{categoryLabel(bridge.category)}
							{#if bridge.is_builtin}
								· built-in
							{/if}
							{#if bridge.version}
								· v{bridge.version}
							{/if}
						</Dialog.Description>
					</div>
				</div>
			</Dialog.Header>

			<div class="space-y-4 text-sm">
				{#if bridge.enabled_globally}
					<section class="space-y-2">
						<h3 class="text-xs font-medium tracking-wide text-muted-foreground uppercase">
							About
						</h3>
						<p class="leading-relaxed text-foreground">
							{bridge.description?.trim() ||
								`${bridge.label} connects Aether to an external service.`}
						</p>
					</section>
					<dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
						<dt class="text-muted-foreground">Feature key</dt>
						<dd><code class="text-xs">{bridge.feature_key}</code></dd>
						<dt class="text-muted-foreground">Category</dt>
						<dd>{categoryLabel(bridge.category)}</dd>
						<dt class="text-muted-foreground">Settings</dt>
						<dd>
							{#if !settingsLoaded}
								<span class="text-muted-foreground">Checking…</span>
							{:else if settingsGroup}
								{settingsGroup.label}
							{:else}
								<span class="text-muted-foreground">No settings group yet</span>
							{/if}
						</dd>
					</dl>
				{:else}
					<p class="leading-relaxed text-muted-foreground">
						This bridge is off for the platform. Enable it to use it from plugins and to configure
						its credentials in settings.
					</p>
					{#if bridge.description?.trim()}
						<p class="leading-relaxed">{bridge.description}</p>
					{/if}
				{/if}

				{#if error}
					<p class="text-sm text-destructive" role="alert">{error}</p>
				{/if}
			</div>

			<div class="flex flex-wrap justify-end gap-2">
				{#if bridge.enabled_globally}
					<Button type="button" variant="outline" disabled={working} onclick={() => setEnabled(false)}>
						{working ? 'Updating…' : 'Disable'}
					</Button>
					{#if settingsGroup}
						<Button type="button" onclick={openSettings}>Open settings</Button>
					{/if}
				{:else}
					<Button type="button" disabled={working} onclick={() => setEnabled(true)}>
						{working ? 'Enabling…' : 'Enable bridge'}
					</Button>
				{/if}
			</div>
		</Dialog.Content>
	{/if}
</Dialog.Root>
