<script lang="ts">
	import { resolve } from '$app/paths';
	import { Button } from '$lib/components/ui/button';
	import type { ErrorAction } from '../types';

	let {
		status,
		title,
		message,
		detail,
		action
	}: {
		status: number;
		title: string;
		message: string;
		detail?: string;
		action?: ErrorAction;
	} = $props();
</script>

<div class="flex min-h-[60vh] items-center justify-center p-6">
	<div class="w-full max-w-md space-y-4 rounded-lg border border-border bg-card p-8 text-card-foreground">
		<p class="text-sm font-medium tracking-widest text-muted-foreground uppercase">Error {status}</p>
		<h1 class="text-2xl font-semibold tracking-tight">{title}</h1>
		<p class="text-sm text-muted-foreground">{message}</p>
		{#if detail}
			<p class="font-mono text-xs break-all text-muted-foreground">{detail}</p>
		{/if}
		<div class="flex gap-2 pt-2">
			<Button href={action?.href ?? resolve('/')}>{action?.label ?? 'Go to the start'}</Button>
			<Button variant="outline" onclick={() => history.back()}>Go back</Button>
		</div>
	</div>
</div>
