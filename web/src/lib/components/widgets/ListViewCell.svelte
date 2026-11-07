<script lang="ts">
	import { Badge } from '$lib/components/ui/badge';
	import { filesOf, legacyImageSrc } from '$lib/media/api';
	import MediaImage from '$lib/media/MediaImage.svelte';

	let {
		value,
		kind,
		labels = {}
	}: {
		value: unknown;
		kind: string;
		/** Display text for stored option values (`on_leave` → `On leave`). */
		labels?: Record<string, string>;
	} = $props();

	function format(raw: unknown, fieldKind: string): string {
		if (raw === null || raw === undefined) return '';
		if (fieldKind === 'boolean') return raw ? 'Yes' : 'No';
		if (typeof raw === 'string' && raw in labels) return labels[raw] ?? raw;
		if (fieldKind === 'date' || fieldKind === 'datetime') {
			const date = new Date(String(raw));
			return Number.isNaN(date.getTime())
				? String(raw)
				: fieldKind === 'date'
					? date.toLocaleDateString()
					: date.toLocaleString();
		}
		if (fieldKind === 'currency' && typeof raw === 'number') {
			return raw.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
		}
		return typeof raw === 'object' ? JSON.stringify(raw) : String(raw);
	}

	const picture = $derived(filesOf(value)[0] ?? null);
	const legacy = $derived(legacyImageSrc(value));
	const files = $derived(filesOf(value));
</script>

{#if kind === 'badge'}
	{#if value !== null && value !== undefined && value !== ''}
		<Badge variant="secondary">{format(value, kind)}</Badge>
	{/if}
{:else if kind === 'image' || kind === 'avatar'}
	{#if picture || legacy}
		<MediaImage
			file={picture}
			{legacy}
			alt=""
			zoomable
			class="size-8 rounded-full object-cover ring-1 ring-border"
		/>
	{:else}
		<span
			class="inline-flex size-8 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground"
		>
			—
		</span>
	{/if}
{:else if kind === 'file'}
	{#if files.length > 0}
		<span class="text-sm text-muted-foreground">
			{files.length === 1 ? files[0]?.name : `${files.length} files`}
		</span>
	{/if}
{:else}
	{format(value, kind)}
{/if}
