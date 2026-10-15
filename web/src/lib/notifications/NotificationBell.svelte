<script lang="ts">
	import { goto } from '$app/navigation';
	import BellIcon from '@lucide/svelte/icons/bell';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { navHref } from '$lib/components/layout/nav';
	import { notificationsStore, type AppNotification } from './notificationsStore.svelte';
	import { timeAgo } from './time';

	const dot: Record<AppNotification['level'], string> = {
		info: 'bg-info',
		success: 'bg-success',
		warning: 'bg-warning',
		error: 'bg-destructive'
	};

	async function open(notification: AppNotification) {
		void notificationsStore.markRead([notification.id]);
		if (notification.link) await goto(navHref(notification.link));
	}
</script>

<DropdownMenu.Root>
	<DropdownMenu.Trigger
		aria-label={notificationsStore.unreachable
			? 'Notifications (server unreachable)'
			: 'Notifications'}
		title={notificationsStore.unreachable ? "Can't reach the server" : undefined}
		class="relative flex size-8 items-center justify-center rounded-md text-region-muted-foreground transition outline-none hover:bg-region-hover hover:text-region-hover-foreground focus-visible:ring-2 focus-visible:ring-region-ring data-[state=open]:bg-region-hover data-[state=open]:text-region-hover-foreground"
	>
		<BellIcon class="size-[18px]" />
		{#if notificationsStore.unreachable}
			<span
				class="absolute -top-0.5 -right-0.5 size-2 rounded-full bg-highlight ring-2 ring-region"
				aria-hidden="true"
			></span>
		{:else if notificationsStore.unread > 0}
			<span
				class="absolute -top-0.5 -right-0.5 flex min-w-4 items-center justify-center rounded-full bg-highlight px-1 text-xs leading-4 font-semibold text-highlight-foreground ring-2 ring-region"
			>
				{notificationsStore.unread > 99 ? '99+' : notificationsStore.unread}
			</span>
		{/if}
	</DropdownMenu.Trigger>
	<DropdownMenu.Content align="end" class="w-96 p-0">
		<div class="flex items-center justify-between border-b border-border px-4 py-2.5">
			<h2 class="text-sm font-semibold">Notifications</h2>
			{#if notificationsStore.unread > 0}
				<button
					type="button"
					class="text-xs text-primary hover:underline"
					onclick={() => notificationsStore.markRead()}
				>
					Mark all as read
				</button>
			{/if}
		</div>
		<ul class="max-h-96 overflow-auto">
			{#each notificationsStore.items as item (item.id)}
				<li>
					<button
						type="button"
						onclick={() => open(item)}
						class="flex w-full gap-3 px-4 py-3 text-left transition hover:bg-accent {item.read
							? ''
							: 'bg-primary/5'}"
					>
						<span
							class="mt-1.5 size-2 shrink-0 rounded-full {item.read
								? 'bg-transparent'
								: dot[item.level]}"
						></span>
						<span class="min-w-0 flex-1">
							<span class="block truncate text-sm {item.read ? '' : 'font-semibold'}"
								>{item.title}</span
							>
							{#if item.body}
								<span class="line-clamp-2 block text-xs text-muted-foreground">{item.body}</span>
							{/if}
							<span class="mt-0.5 block text-xs text-muted-foreground">
								{timeAgo(item.created_at)}{item.source === 'kernel' ? '' : ` · ${item.source}`}
							</span>
						</span>
					</button>
				</li>
			{:else}
				<li class="px-4 py-8 text-center text-sm text-muted-foreground">You're all caught up.</li>
			{/each}
		</ul>
	</DropdownMenu.Content>
</DropdownMenu.Root>
