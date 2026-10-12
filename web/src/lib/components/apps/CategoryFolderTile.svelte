<script lang="ts">
	import FolderIcon from '@lucide/svelte/icons/folder';
	import * as Card from '$lib/components/ui/card/index.js';

	let {
		label,
		count,
		onclick
	}: {
		label: string;
		count: number;
		onclick: () => void;
	} = $props();
</script>

<button type="button" class="group block w-full outline-none" {onclick}>
	<Card.Root
		size="sm"
		class="h-full transition-colors group-hover:bg-accent/40 group-focus-visible:ring-2 group-focus-visible:ring-ring"
	>
		<Card.Content
			class="flex flex-col items-center gap-2.5 px-(--card-spacing) pt-(--card-spacing) text-center"
		>
			<span
				class="flex size-12 items-center justify-center bg-amber-500/10 text-amber-700 ring-1 ring-amber-500/20 transition group-hover:bg-amber-500/15 dark:text-amber-400"
				aria-hidden="true"
			>
				<FolderIcon class="size-6" />
			</span>
			<div class="max-w-full space-y-0.5">
				<Card.Title class="truncate text-[13px] font-medium">{label}</Card.Title>
				<Card.Description class="text-[11px] leading-snug">
					{count} app{count === 1 ? '' : 's'}
				</Card.Description>
			</div>
		</Card.Content>
	</Card.Root>
</button>
