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

<!-- A bolder variant: the status is the hero, drawn in the theme's primary colour. -->
<div class="flex min-h-[70vh] items-center justify-center bg-gradient-to-b from-muted/60 to-background p-6">
	<div class="w-full max-w-xl space-y-5 text-center">
		<p class="text-8xl font-black tracking-tighter text-primary">{status}</p>
		<h1 class="text-3xl font-semibold tracking-tight">{title}</h1>
		<p class="mx-auto max-w-md text-muted-foreground">{message}</p>
		{#if detail}
			<p class="font-mono text-xs break-all text-muted-foreground">{detail}</p>
		{/if}
		<div class="flex justify-center gap-3 pt-2">
			<Button href={action?.href ?? resolve('/')} size="lg">{action?.label ?? 'Take me home'}</Button>
			<Button variant="ghost" size="lg" onclick={() => history.back()}>Go back</Button>
		</div>
	</div>
</div>
