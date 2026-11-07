<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { downloadMedia, formatSize, uploadMedia, type MediaFile } from '$lib/media/api';
	import { getPageState } from '$lib/pages/pageContext.svelte';
	import { ago } from '$lib/pages/files';

	/** The versions of a file: download one, make an old one current again, or upload a new one. */
	let {
		open = $bindable(false),
		file,
		name,
		versions,
		restore,
		newVersion,
		canEdit,
		guard = false,
		onchange
	}: {
		open?: boolean;
		file: string;
		name: string;
		versions: string;
		restore?: string;
		newVersion?: string;
		canEdit: boolean;
		/** Guard the uploaded file from the start (see `uploadMedia`). */
		guard?: boolean;
		onchange?: () => void;
	} = $props();

	const page = getPageState();
	interface Version {
		version: number;
		size: number;
		uploaded_by?: string;
		at?: string;
		current: boolean;
		media?: MediaFile;
	}
	let rows = $state<Version[]>([]);
	let error = $state<string | null>(null);
	let busy = $state(false);
	let input = $state<HTMLInputElement | null>(null);

	async function load() {
		if (!page) return;
		const r = await page.call(versions, { file });
		rows = r.ok && Array.isArray(r.data) ? (r.data as Version[]) : [];
	}

	$effect(() => {
		if (open) {
			error = null;
			void load();
		}
	});

	async function make(version: number) {
		if (!page || !restore) return;
		busy = true;
		const r = await page.call(restore, { file, version });
		busy = false;
		if (!r.ok) error = r.error;
		await load();
		onchange?.();
	}

	async function upload(list: FileList | null) {
		const picked = list?.[0];
		if (!picked || !page || !newVersion) return;
		busy = true;
		error = null;
		try {
			const media = await uploadMedia(picked, guard ? page.plugin : undefined);
			const r = await page.call(newVersion, { file, media });
			if (!r.ok) error = r.error;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The upload failed.';
		}
		busy = false;
		if (input) input.value = '';
		await load();
		onchange?.();
	}

	async function save(v: Version) {
		if (!v.media) return;
		try {
			await downloadMedia({ ...v.media, name });
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'The download failed.';
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-lg">
		<Dialog.Header><Dialog.Title>Versions of “{name}”</Dialog.Title></Dialog.Header>
		{#if error}<p class="text-sm text-destructive" role="alert">{error}</p>{/if}
		<ul
			class="max-h-72 divide-y divide-border overflow-y-auto rounded-md border border-border"
			data-part="versions"
		>
			{#each rows as v (v.version)}
				<li class="flex items-center gap-2 px-3 py-2 text-sm">
					<span class="font-medium">v{v.version}</span>
					{#if v.current}<span class="rounded bg-primary/10 px-1.5 text-xs text-primary"
							>current</span
						>{/if}
					<span class="min-w-0 flex-1 truncate text-xs text-muted-foreground"
						>{v.uploaded_by ?? ''} · {ago(v.at)} · {formatSize(v.size)}</span
					>
					<Button size="xs" variant="ghost" onclick={() => void save(v)}>Download</Button>
					{#if canEdit && !v.current && restore}
						<Button size="xs" variant="outline" disabled={busy} onclick={() => void make(v.version)}
							>Make current</Button
						>
					{/if}
				</li>
			{/each}
		</ul>
		<Dialog.Footer>
			{#if canEdit && newVersion}
				<input
					bind:this={input}
					type="file"
					class="sr-only"
					onchange={(e) => void upload(e.currentTarget.files)}
				/>
				<Button type="button" variant="outline" disabled={busy} onclick={() => input?.click()}
					>Upload a new version</Button
				>
			{/if}
			<Button type="button" onclick={() => (open = false)}>Close</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
