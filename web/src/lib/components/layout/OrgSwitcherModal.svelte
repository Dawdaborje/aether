<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Button } from '$lib/components/ui/button';
	import { orgStore, type OrgChoice } from '$lib/org/orgStore.svelte';

	const required = $derived(orgStore.selectionRequired);
	const hint = $derived(
		orgStore.mode === 'header'
			? 'This browser will remember your choice and send it with every request.'
			: 'Your choice is saved to your session.'
	);

	// When the user must choose, only the organizations they belong to are offered.
	// Otherwise a developer also sees the rest, below their own.
	const yours = $derived(orgStore.memberships);
	const others = $derived(required ? [] : orgStore.organizations.filter((org) => !org.member));
	const sections = $derived(
		[
			{ title: others.length > 0 ? 'Your organizations' : null, orgs: yours },
			{ title: 'Other organizations', orgs: others }
		].filter((section) => section.orgs.length > 0)
	);
</script>

{#snippet orgButton(org: OrgChoice)}
	<li>
		<Button
			variant={org.db_name === orgStore.current ? 'default' : 'outline'}
			class="h-auto w-full justify-between px-4 py-3 text-left"
			disabled={orgStore.switching}
			onclick={() => orgStore.choose(org.db_name)}
		>
			<span class="flex flex-col items-start normal-case tracking-normal">
				<span class="text-sm font-medium">{org.name}</span>
				{#if org.db_name.toLowerCase() !== org.name.toLowerCase()}
					<span class="font-mono text-xs opacity-70">{org.db_name}</span>
				{/if}
			</span>
			{#if org.db_name === orgStore.current}
				<span class="text-xs tracking-wide uppercase">Current</span>
			{/if}
		</Button>
	</li>
{/snippet}

<Dialog.Root
	open={orgStore.modalOpen}
	onOpenChange={(open) => {
		if (!open) orgStore.closeSwitcher();
	}}
>
	<Dialog.Content
		showCloseButton={!required}
		interactOutsideBehavior={required ? 'ignore' : 'close'}
		escapeKeydownBehavior={required ? 'ignore' : 'close'}
	>
		<Dialog.Header>
			<Dialog.Title>{required ? 'Choose an organization' : 'Switch organization'}</Dialog.Title>
			<Dialog.Description>
				{required
					? 'You belong to more than one organization. Pick the one to work in.'
					: 'Pick the organization to work in.'}
				{hint}
			</Dialog.Description>
		</Dialog.Header>

		{#each sections as section (section.title ?? 'all')}
			<section class="space-y-2">
				{#if section.title}
					<h3 class="text-xs font-semibold tracking-widest text-muted-foreground uppercase">
						{section.title}
					</h3>
				{/if}
				<ul class="space-y-2" aria-label={section.title ?? 'Organizations'}>
					{#each section.orgs as org (org.db_name)}
						{@render orgButton(org)}
					{/each}
				</ul>
			</section>
		{/each}

		{#if orgStore.error}
			<p class="text-sm text-destructive" role="alert">{orgStore.error}</p>
		{/if}
	</Dialog.Content>
</Dialog.Root>
