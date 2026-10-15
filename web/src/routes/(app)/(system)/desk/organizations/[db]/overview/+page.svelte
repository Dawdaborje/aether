<script lang="ts">
	import { onMount } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import OrgLogo from '$lib/components/org/OrgLogo.svelte';
	import {
		clearOrganizationLogo,
		fetchOrganizations,
		updateOrganization,
		uploadOrganizationLogo,
		type OrganizationProfile
	} from '$lib/org/api';
	import { getOrgDetailContext } from '$lib/org/orgDetailContext';
	import { resolve } from '$app/paths';

	let { params }: { params: { db: string } } = $props();

	const detail = getOrgDetailContext();

	let name = $state('');
	let description = $state('');
	let parent = $state('');
	let parents = $state<OrganizationProfile[]>([]);
	let working = $state(false);
	let logoWorking = $state(false);
	let message = $state<string | null>(null);
	let failed = $state<string | null>(null);
	let logoInput: HTMLInputElement | undefined = $state();

	function syncFrom(org: OrganizationProfile | null) {
		if (!org) return;
		name = org.name;
		description = org.description ?? '';
		parent = org.parent_db_name ?? '';
	}

	onMount(async () => {
		syncFrom(detail.organization);
		try {
			parents = (await fetchOrganizations()).filter((org) => org.db_name !== params.db);
		} catch {
			parents = [];
		}
	});

	$effect(() => {
		syncFrom(detail.organization);
	});

	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (working) return;
		working = true;
		failed = null;
		message = null;
		try {
			const next = await updateOrganization(params.db, {
				name,
				description,
				parent,
				set: ['description', 'parent']
			});
			detail.setOrganization(next);
			message = 'Saved.';
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Could not save.';
		} finally {
			working = false;
		}
	}

	async function onLogoSelected(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file || logoWorking) return;
		logoWorking = true;
		failed = null;
		message = null;
		try {
			const next = await uploadOrganizationLogo(params.db, file);
			detail.setOrganization(next);
			message = 'Logo updated.';
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Could not upload the logo.';
		} finally {
			logoWorking = false;
		}
	}

	async function removeLogo() {
		if (logoWorking) return;
		logoWorking = true;
		failed = null;
		message = null;
		try {
			const next = await clearOrganizationLogo(params.db);
			detail.setOrganization(next);
			message = 'Logo removed.';
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Could not remove the logo.';
		} finally {
			logoWorking = false;
		}
	}
</script>

<div class="grid gap-8 lg:grid-cols-[minmax(0,1fr)_16rem]">
	<form class="space-y-5" onsubmit={save}>
		<div class="space-y-1.5">
			<Label for="org-detail-name">Name</Label>
			<Input id="org-detail-name" bind:value={name} autocomplete="off" />
		</div>
		<div class="space-y-1.5">
			<Label for="org-detail-description">Description</Label>
			<textarea
				id="org-detail-description"
				bind:value={description}
				rows="4"
				class="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:outline-none"
				placeholder="What this organization is for"
			></textarea>
		</div>
		<div class="space-y-1.5">
			<Label for="org-detail-parent">Parent organization</Label>
			<select
				id="org-detail-parent"
				bind:value={parent}
				class="border-input bg-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 text-sm focus-visible:ring-2 focus-visible:outline-none"
			>
				<option value="">None</option>
				{#each parents as org (org.db_name)}
					<option value={org.db_name}>{org.name} ({org.db_name})</option>
				{/each}
			</select>
			<p class="text-xs text-muted-foreground">
				Optional. Use this when the org sits under a holding company or regional parent.
			</p>
		</div>

		{#if failed}
			<p class="text-sm text-destructive" role="alert">{failed}</p>
		{/if}
		{#if message}
			<p class="text-sm text-primary" role="status">{message}</p>
		{/if}

		<div class="flex flex-wrap gap-2">
			<Button type="submit" disabled={working}>{working ? 'Saving…' : 'Save changes'}</Button>
			{#if detail.organization?.parent_db_name}
				<Button
					type="button"
					variant="outline"
					href={resolve(`/desk/organizations/${detail.organization.parent_db_name}/overview`)}
				>
					Open parent
				</Button>
			{/if}
		</div>
	</form>

	<aside class="space-y-3 rounded-lg border border-border p-4">
		<p class="text-sm font-medium">Logo</p>
		<p class="text-xs text-muted-foreground">
			Upload an image for this organization. Until then, a default mark is shown.
		</p>
		<button
			type="button"
			class="group mx-auto block rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-60"
			disabled={logoWorking}
			onclick={() => logoInput?.click()}
			aria-label={detail.organization?.logo_url ? 'Change organization logo' : 'Upload organization logo'}
		>
			<span class="relative block">
				<OrgLogo
					logoUrl={detail.organization?.logo_url}
					name={detail.organization?.name ?? params.db}
					cacheKey={detail.organization?.date_updated}
					class="size-28 rounded-xl transition group-hover:ring-primary/30"
				/>
				<span
					class="pointer-events-none absolute inset-0 flex items-end justify-center rounded-xl bg-black/45 pb-2 text-xs font-medium text-white opacity-0 transition group-hover:opacity-100"
				>
					{logoWorking ? 'Uploading…' : detail.organization?.logo_url ? 'Change' : 'Upload'}
				</span>
			</span>
		</button>
		<input
			bind:this={logoInput}
			type="file"
			accept="image/png,image/jpeg,image/webp,image/gif,image/svg+xml,image/x-icon"
			class="sr-only"
			disabled={logoWorking}
			onchange={onLogoSelected}
		/>
		<div class="flex flex-col gap-2">
			<Button
				type="button"
				variant="outline"
				size="sm"
				class="w-full"
				disabled={logoWorking}
				onclick={() => logoInput?.click()}
			>
				{logoWorking
					? 'Uploading…'
					: detail.organization?.logo_url
						? 'Replace image'
						: 'Upload image'}
			</Button>
			{#if detail.organization?.logo_url}
				<Button
					type="button"
					variant="ghost"
					size="sm"
					class="w-full"
					disabled={logoWorking}
					onclick={removeLogo}
				>
					Remove logo
				</Button>
			{/if}
		</div>
		<p class="text-xs text-muted-foreground">PNG, JPG, WebP, GIF, SVG or ICO, up to 2 MB.</p>

		<dl class="space-y-2 border-t border-border pt-3 text-xs">
			<div>
				<dt class="text-muted-foreground">Identifier</dt>
				<dd class="font-mono">{params.db}</dd>
			</div>
			<div>
				<dt class="text-muted-foreground">Owner</dt>
				<dd>{detail.organization?.owner_login ?? '—'}</dd>
			</div>
			<div>
				<dt class="text-muted-foreground">Members</dt>
				<dd>{detail.organization?.member_count ?? 0}</dd>
			</div>
			<div>
				<dt class="text-muted-foreground">Created</dt>
				<dd class="break-all">{detail.organization?.date_created || '—'}</dd>
			</div>
		</dl>
	</aside>
</div>
