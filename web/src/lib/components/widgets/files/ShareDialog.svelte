<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { navHref } from '$lib/components/layout/nav';
	import { getPageState } from '$lib/pages/pageContext.svelte';

	/**
	 * Who an item is shared with. `shares` lists them (`[{ id, name, level }]`), `share` adds or changes one
	 * (`{ kind, id, user, level }`), `unshare` removes one; `people` lists the people to choose from.
	 */
	let {
		open = $bindable(false),
		kind,
		id,
		name,
		shares,
		share,
		unshare,
		people,
		links,
		createLink,
		revokeLink,
		onchange
	}: {
		open?: boolean;
		kind: string;
		id: string;
		name: string;
		shares: string;
		share: string;
		unshare: string;
		people?: string;
		/** Sharing by link: `links` lists them (`[{ id, path, text, expired }]`), `createLink` makes one, `revokeLink` takes one back. */
		links?: string;
		createLink?: string;
		revokeLink?: string;
		onchange?: () => void;
	} = $props();

	const page = getPageState();
	let current = $state<{ id: string; name: string; level: string }[]>([]);
	let options = $state<{ id: string; name: string }[]>([]);
	let who = $state('');
	let level = $state('view');
	let error = $state<string | null>(null);
	let busy = $state(false);

	type LinkRow = { id: string; path: string; text: string; expired?: boolean };
	let linkRows = $state<LinkRow[]>([]);
	let copied = $state('');

	const linksOn = $derived(Boolean(links && createLink && revokeLink));

	async function load() {
		if (!page) return;
		const r = await page.call(shares, { kind, id });
		current = r.ok && Array.isArray(r.data) ? (r.data as typeof current) : [];
		if (linksOn) {
			const l = await page.call(links!, { kind, id });
			linkRows = l.ok && Array.isArray(l.data) ? (l.data as LinkRow[]) : [];
		}
	}

	const fullUrl = (path: string) => new URL(navHref(path), window.location.origin).toString();

	async function makeLink() {
		if (!page || !createLink) return;
		busy = true;
		const r = await page.call(createLink, { kind, id });
		busy = false;
		if (!r.ok) {
			error = r.error;
			return;
		}
		error = null;
		await load();
	}

	async function copy(row: LinkRow) {
		try {
			await navigator.clipboard.writeText(fullUrl(row.path));
			copied = row.id;
		} catch {
			error = 'Could not copy; select the link and copy it by hand.';
		}
	}

	async function revoke(row: LinkRow) {
		if (!page || !revokeLink) return;
		const r = await page.call(revokeLink, { id: row.id });
		if (!r.ok) error = r.error;
		await load();
	}

	$effect(() => {
		if (!open || !page) return;
		error = null;
		copied = '';
		who = '';
		level = 'view';
		void load();
		if (people)
			void page.call(people, {}).then((r) => {
				options = r.ok && Array.isArray(r.data) ? (r.data as typeof options) : [];
			});
	});

	async function add() {
		if (!who || !page) return;
		busy = true;
		const r = await page.call(share, { kind, id, user: who, level });
		busy = false;
		if (!r.ok) {
			error = r.error;
			return;
		}
		error = null;
		who = '';
		await load();
		onchange?.();
	}

	async function change(user: string, next: string) {
		if (!page) return;
		const r = await page.call(share, { kind, id, user, level: next });
		if (!r.ok) error = r.error;
		await load();
		onchange?.();
	}

	async function remove(user: string) {
		if (!page) return;
		const r = await page.call(unshare, { kind, id, user });
		if (!r.ok) error = r.error;
		await load();
		onchange?.();
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-lg">
		<Dialog.Header>
			<Dialog.Title>Share “{name}”</Dialog.Title>
			<Dialog.Description>
				{kind === 'folder'
					? 'Everything inside the folder is shared too.'
					: 'People you share with can open it.'}
			</Dialog.Description>
		</Dialog.Header>
		<form
			class="flex gap-2"
			onsubmit={(e) => {
				e.preventDefault();
				void add();
			}}
		>
			<select
				bind:value={who}
				aria-label="Person"
				class="h-9 min-w-0 flex-1 rounded-md border border-input bg-transparent pr-7 pl-2 text-sm"
			>
				<option value="">Choose a person…</option>
				<option value="org">Everyone in the organization</option>
				{#each options as person (person.id)}<option value={person.id}>{person.name}</option>{/each}
			</select>
			<select
				bind:value={level}
				aria-label="Access"
				class="h-9 rounded-md border border-input bg-transparent pr-7 pl-2 text-sm"
			>
				<option value="view">Can view</option>
				<option value="edit">Can edit</option>
			</select>
			<Button type="submit" disabled={!who || busy}>Share</Button>
		</form>
		{#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
		<ul
			class="max-h-60 divide-y divide-border overflow-y-auto rounded-md border border-border"
			data-part="shares"
		>
			{#each current as entry (entry.id)}
				<li class="flex items-center gap-2 px-3 py-2 text-sm">
					<span class="min-w-0 flex-1 truncate">{entry.name}</span>
					<select
						value={entry.level}
						aria-label="Access for {entry.name}"
						class="h-8 rounded-md border border-input bg-transparent pr-6 pl-2 text-xs"
						onchange={(e) => void change(entry.id, e.currentTarget.value)}
					>
						<option value="view">Can view</option>
						<option value="edit">Can edit</option>
					</select>
					<button
						type="button"
						class="text-muted-foreground hover:text-foreground"
						aria-label="Stop sharing with {entry.name}"
						onclick={() => void remove(entry.id)}><AppIcon name="close" class="size-4" /></button
					>
				</li>
			{:else}
				<li class="px-3 py-3 text-sm text-muted-foreground">Not shared with anyone yet.</li>
			{/each}
		</ul>
		{#if linksOn}
			<section class="space-y-2" data-part="links">
				<div class="flex items-center gap-2">
					<h3 class="flex-1 text-sm font-medium">Link</h3>
					<Button
						type="button"
						size="sm"
						variant="outline"
						disabled={busy}
						onclick={() => void makeLink()}>Create a link</Button
					>
				</div>
				<p class="text-xs text-muted-foreground">
					Anyone in the organization who has the link can view this.
				</p>
				<ul class="divide-y divide-border rounded-md border border-border">
					{#each linkRows as row (row.id)}
						<li class="flex items-center gap-2 px-3 py-2 text-xs" data-part="link">
							<span class="min-w-0 flex-1">
								<span class="block truncate font-mono" data-part="link-url"
									>{fullUrl(row.path)}</span
								>
								<span class="text-muted-foreground">{row.expired ? 'Expired' : row.text}</span>
							</span>
							<Button type="button" size="sm" variant="outline" onclick={() => void copy(row)}
								>{copied === row.id ? 'Copied' : 'Copy'}</Button
							>
							<button
								type="button"
								class="text-muted-foreground hover:text-foreground"
								aria-label="Take this link back"
								onclick={() => void revoke(row)}><AppIcon name="close" class="size-4" /></button
							>
						</li>
					{:else}
						<li class="px-3 py-3 text-xs text-muted-foreground">No links.</li>
					{/each}
				</ul>
			</section>
		{/if}
		<Dialog.Footer><Button type="button" onclick={() => (open = false)}>Done</Button></Dialog.Footer
		>
	</Dialog.Content>
</Dialog.Root>
