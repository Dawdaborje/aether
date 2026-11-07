<script lang="ts">
	import type { Snippet } from 'svelte';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { cn } from '$lib/utils.js';

	let {
		icon,
		title,
		description,
		class: className,
		children
	}: {
		/** An icon name (see `$lib/icons`). */
		icon?: string;
		title: string;
		description?: string;
		class?: string;
		/** Optional actions shown under the text. */
		children?: Snippet;
	} = $props();
</script>

<div
	data-slot="empty"
	class={cn(
		'flex w-full flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-border px-6 py-16 text-center',
		className
	)}
>
	{#if icon}
		<div
			class="flex size-12 items-center justify-center rounded-full bg-muted text-muted-foreground"
		>
			<AppIcon name={icon} class="size-6" />
		</div>
	{/if}
	<div class="space-y-1">
		<h3 class="text-sm font-medium">{title}</h3>
		{#if description}<p class="max-w-2xl text-sm text-muted-foreground">{description}</p>{/if}
	</div>
	{@render children?.()}
</div>
