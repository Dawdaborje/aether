<script lang="ts">
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import type { Snippet } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { statusOf } from '$lib/errors/report';

	/**
	 * What a page shows, in the middle of its content area, when the data it needs could not be
	 * loaded. The icon and title follow the status at the end of `message` (`… (502).`): the
	 * server being unreachable reads differently from a missing permission. `children` adds
	 * actions next to "Try again" (a way back to the list).
	 */
	let {
		message,
		title,
		onRetry,
		retrying = false,
		children
	}: {
		message: string;
		title?: string;
		onRetry?: () => void | Promise<void>;
		retrying?: boolean;
		children?: Snippet;
	} = $props();

	type Look = { icon: string; title: string; hint: string; tone: string };

	const look = $derived.by((): Look => {
		const status = statusOf(message);
		// No status and a fetch failure's wording: the request never got an answer.
		const unanswered = status === 0 && /network|failed to fetch|load failed/i.test(message);
		if (unanswered || status === 502 || status === 503 || status === 504)
			return {
				icon: 'cloud-off',
				title: 'Server unreachable',
				hint: 'Aether did not answer. It may be restarting; try again in a moment.',
				tone: 'bg-destructive/10 text-destructive'
			};
		if (status === 401 || status === 403)
			return {
				icon: 'lock',
				title: 'No access',
				hint: 'Your account is not allowed to see this.',
				tone: 'bg-warning/10 text-warning'
			};
		if (status === 404)
			return {
				icon: 'search-x',
				title: 'Not found',
				hint: 'It may have been removed or renamed.',
				tone: 'bg-muted text-muted-foreground'
			};
		if (status === 429)
			return {
				icon: 'clock',
				title: 'Too many requests',
				hint: 'Wait a few seconds, then try again.',
				tone: 'bg-warning/10 text-warning'
			};
		return {
			icon: 'warning',
			title: 'Something went wrong',
			hint: '',
			tone: 'bg-destructive/10 text-destructive'
		};
	});
</script>

<div
	class="flex w-full flex-1 items-center justify-center px-4 py-16"
	role="alert"
	aria-live="polite"
>
	<div class="flex max-w-md flex-col items-center gap-4 text-center">
		<div
			class={`flex size-14 items-center justify-center rounded-2xl ${look.tone}`}
			aria-hidden="true"
		>
			<AppIcon name={look.icon} class="size-7" />
		</div>
		<div class="space-y-1.5">
			<h2 class="text-lg font-semibold tracking-tight">{title ?? look.title}</h2>
			<p class="text-sm text-muted-foreground">{message}</p>
			{#if look.hint}
				<p class="text-sm text-muted-foreground">{look.hint}</p>
			{/if}
		</div>
		{#if onRetry || children}
			<div class="flex flex-wrap justify-center gap-2">
				{#if onRetry}
					<Button size="sm" disabled={retrying} onclick={() => void onRetry()}>
						<AppIcon
							name="refresh"
							class={retrying ? 'size-4 animate-spin' : 'size-4'}
							data-icon="inline-start"
						/>
						{retrying ? 'Trying…' : 'Try again'}
					</Button>
				{/if}
				{@render children?.()}
			</div>
		{/if}
	</div>
</div>
