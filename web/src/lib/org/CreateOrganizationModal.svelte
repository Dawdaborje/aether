<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import {
		createOrganization,
		identifierFor,
		type CreatedOrganization
	} from '$lib/org/createOrganization';
	import type { OrganizationProfile } from '$lib/org/api';

	let {
		open = $bindable(false),
		parents = [],
		onCreated
	}: {
		open: boolean;
		/** Existing orgs offered as parent. */
		parents?: OrganizationProfile[];
		onCreated?: () => void | Promise<void>;
	} = $props();

	let name = $state('');
	/** The organization's own contact address (not the first user's). */
	let orgEmail = $state('');
	let identifier = $state('');
	/** The identifier follows the name until the person edits it. */
	let identifierEdited = $state(false);
	let description = $state('');
	let parent = $state('');
	let mode = $state<'none' | 'new' | 'existing'>('none');
	let username = $state('');
	let email = $state('');
	let password = $state('');
	let login = $state('');
	let working = $state(false);
	let failed = $state<string | null>(null);
	let created = $state<CreatedOrganization | null>(null);

	// Start empty each time the dialog opens.
	$effect(() => {
		if (!open) return;
		name = orgEmail = identifier = description = parent = username = email = password = login = '';
		identifierEdited = false;
		mode = 'none';
		failed = null;
		created = null;
	});

	$effect(() => {
		if (!identifierEdited) identifier = identifierFor(name);
	});

	const ready = $derived(
		name.trim() !== '' &&
			/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(orgEmail.trim()) &&
			(mode === 'none' ||
				(mode === 'existing'
					? login.trim() !== ''
					: username.trim().length >= 3 && email.includes('@') && password.length >= 8))
	);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (!ready || working) return;
		working = true;
		failed = null;
		try {
			created = await createOrganization({
				name,
				email: orgEmail,
				identifier,
				description,
				parent: parent || undefined,
				member:
					mode === 'none'
						? null
						: mode === 'existing'
							? { mode, login }
							: { mode, username, email, password }
			});
			await orgStore.load();
			await onCreated?.();
		} catch (error) {
			failed = error instanceof Error ? error.message : 'The organization could not be created.';
		} finally {
			working = false;
		}
	}

	async function enter(dbName: string) {
		await orgStore.choose(dbName);
		if (!orgStore.error) {
			open = false;
			await goto(resolve('/apps'));
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="max-w-lg">
		{#if created}
			<Dialog.Header>
				<Dialog.Title>{created.organization.name} is ready</Dialog.Title>
				<Dialog.Description>
					Its database and files are set up. Open it to add a logo and description, or
					install apps from the desk.
				</Dialog.Description>
			</Dialog.Header>
			<p class="text-xs text-muted-foreground">
				Identifier <span class="font-mono">{created.organization.db_name}</span> · files in
				<span class="font-mono break-all">{created.files}</span>
			</p>
			<Dialog.Footer>
				<Button
					variant="outline"
					href={resolve(`/desk/organizations/${created.organization.db_name}/overview`)}
					onclick={() => (open = false)}>Open details</Button
				>
				{#if orgStore.mode !== 'address'}
					<Button disabled={orgStore.switching} onclick={() => enter(created!.organization.db_name)}>
						Enter organization
					</Button>
				{/if}
			</Dialog.Footer>
		{:else}
			<Dialog.Header>
				<Dialog.Title>Create organization</Dialog.Title>
				<Dialog.Description>
					An organization has its own database and files. It can start with an owner, or empty.
				</Dialog.Description>
			</Dialog.Header>
			<form class="space-y-5" onsubmit={submit}>
				<div class="space-y-3">
					<div class="space-y-1.5">
						<Label for="org-name">Name</Label>
						<Input id="org-name" bind:value={name} placeholder="Acme Corporation" autocomplete="off" />
					</div>
					<div class="space-y-1.5">
						<Label for="org-contact-email">Email</Label>
						<Input
							id="org-contact-email"
							type="email"
							bind:value={orgEmail}
							placeholder="hello@acme.com"
							autocomplete="off"
							required
						/>
						<p class="text-xs text-muted-foreground">
							The organization's contact address. It becomes the email of its default company.
						</p>
					</div>
					<div class="space-y-1.5">
						<Label for="org-identifier">Identifier</Label>
						<Input
							id="org-identifier"
							bind:value={identifier}
							oninput={() => (identifierEdited = true)}
							class="font-mono"
							autocomplete="off"
						/>
						<p class="text-xs text-muted-foreground">
							Names its database and folder. Letters, digits and underscores; it cannot be changed
							later.
						</p>
					</div>
					<div class="space-y-1.5">
						<Label for="org-description">Description</Label>
						<textarea
							id="org-description"
							bind:value={description}
							rows="2"
							class="border-input bg-background ring-offset-background placeholder:text-muted-foreground focus-visible:ring-ring flex w-full rounded-md border px-3 py-2 text-sm focus-visible:ring-2 focus-visible:outline-none"
							placeholder="What this organization is for"
						></textarea>
					</div>
					{#if parents.length > 0}
						<div class="space-y-1.5">
							<Label for="org-parent">Parent organization</Label>
							<select
								id="org-parent"
								bind:value={parent}
								class="border-input bg-background focus-visible:ring-ring flex h-10 w-full rounded-md border px-3 text-sm focus-visible:ring-2 focus-visible:outline-none"
							>
								<option value="">None</option>
								{#each parents as org (org.db_name)}
									<option value={org.db_name}>{org.name} ({org.db_name})</option>
								{/each}
							</select>
						</div>
					{/if}
				</div>

				<fieldset class="space-y-3">
					<legend class="text-sm font-semibold">First member (owner) · optional</legend>
					<div class="inline-flex rounded-md border border-border p-0.5 text-sm">
						{#each [['none', 'No member yet'], ['new', 'New user'], ['existing', 'Existing user']] as const as [value, label] (value)}
							<button
								type="button"
								onclick={() => (mode = value)}
								aria-pressed={mode === value}
								class="rounded px-3 py-1 transition aria-pressed:bg-primary aria-pressed:text-primary-foreground"
							>
								{label}
							</button>
						{/each}
					</div>
					{#if mode === 'none'}
						<p class="text-xs text-muted-foreground">
							The organization is created empty, with no owner. Add members later from its
							details page.
						</p>
					{:else if mode === 'new'}
						<div class="grid gap-3 sm:grid-cols-2">
							<div class="space-y-1.5">
								<Label for="org-username">Username</Label>
								<Input id="org-username" bind:value={username} autocomplete="off" />
							</div>
							<div class="space-y-1.5">
								<Label for="org-email">Email</Label>
								<Input id="org-email" type="email" bind:value={email} autocomplete="off" />
							</div>
						</div>
						<div class="space-y-1.5">
							<Label for="org-password">Password</Label>
							<Input
								id="org-password"
								type="password"
								bind:value={password}
								autocomplete="new-password"
							/>
							<p class="text-xs text-muted-foreground">At least 8 characters.</p>
						</div>
					{:else}
						<div class="space-y-1.5">
							<Label for="org-login">Username or email</Label>
							<Input id="org-login" bind:value={login} autocomplete="off" />
						</div>
					{/if}
				</fieldset>

				{#if failed}
					<p class="text-sm text-destructive" role="alert">{failed}</p>
				{/if}
				<Dialog.Footer>
					<Button type="button" variant="outline" onclick={() => (open = false)}>Cancel</Button>
					<Button type="submit" disabled={!ready || working}>
						{working ? 'Creating…' : 'Create organization'}
					</Button>
				</Dialog.Footer>
			</form>
		{/if}
	</Dialog.Content>
</Dialog.Root>
