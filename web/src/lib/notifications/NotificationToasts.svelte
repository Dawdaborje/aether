<script lang="ts">
	import { goto } from '$app/navigation';
	import XIcon from '@lucide/svelte/icons/x';
	import { navHref } from '$lib/components/layout/nav';
	import { notificationsStore, type AppNotification } from './notificationsStore.svelte';

	const bar: Record<AppNotification['level'], string> = {
		info: 'bg-primary',
		success: 'bg-emerald-500',
		warning: 'bg-amber-500',
		error: 'bg-red-500'
	};

	async function open(toast: AppNotification) {
		notificationsStore.dismissToast(toast.id);
		void notificationsStore.markRead([toast.id]);
		if (toast.link) await goto(navHref(toast.link));
	}
</script>

<div
	class="pointer-events-none fixed right-4 bottom-4 z-50 flex w-80 flex-col gap-2"
	aria-live="polite"
>
	{#each notificationsStore.toasts as toast (toast.id)}
		<div
			class="pointer-events-auto flex overflow-hidden rounded-lg border border-border bg-popover text-popover-foreground shadow-lg"
		>
			<span class="w-1 shrink-0 {bar[toast.level]}"></span>
			<button
				type="button"
				onclick={() => open(toast)}
				class="min-w-0 flex-1 px-3 py-2.5 text-left"
			>
				<span class="block truncate text-sm font-semibold">{toast.title}</span>
				{#if toast.body}
					<span class="line-clamp-2 block text-xs text-muted-foreground">{toast.body}</span>
				{/if}
			</button>
			<button
				type="button"
				aria-label="Dismiss"
				onclick={() => notificationsStore.dismissToast(toast.id)}
				class="px-2 text-muted-foreground hover:text-foreground"
			>
				<XIcon class="size-4" />
			</button>
		</div>
	{/each}
</div>
