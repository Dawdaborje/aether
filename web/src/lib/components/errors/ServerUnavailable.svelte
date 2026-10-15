<script lang="ts">
	import CloudOffIcon from '@lucide/svelte/icons/cloud-off';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import { Button } from '$lib/components/ui/button';

	let {
		/** Full-page takeover vs sticky strip while the shell is already up. */
		variant = 'page',
		/** Auto-reconnect loop is running. */
		retrying = true,
		/** How many failed bootstrap attempts so far (optional). */
		attempt = 0,
		/** Manual retry; defaults to a full reload. */
		onRetry
	}: {
		variant?: 'page' | 'banner';
		retrying?: boolean;
		attempt?: number;
		onRetry?: () => void;
	} = $props();

	function retry() {
		if (onRetry) onRetry();
		else window.location.reload();
	}
</script>

{#if variant === 'banner'}
	<div
		class="sticky top-0 z-50 border-b border-border bg-card px-4 py-3 text-card-foreground shadow-sm"
		role="alert"
		aria-live="polite"
	>
		<div class="mx-auto flex max-w-5xl flex-wrap items-center justify-between gap-3">
			<div class="flex min-w-0 items-start gap-3">
				<div
					class="mt-0.5 flex size-9 shrink-0 items-center justify-center rounded-md bg-destructive/10 text-destructive"
					aria-hidden="true"
				>
					<CloudOffIcon class="size-4" />
				</div>
				<div class="min-w-0 space-y-0.5">
					<p class="text-sm font-semibold tracking-tight">Server unreachable</p>
					<p class="text-sm text-muted-foreground">
						{#if retrying}
							Connection lost. Reconnecting
							{#if attempt > 0}
								<span class="tabular-nums">(attempt {attempt})</span>
							{/if}…
						{:else}
							The API did not answer. Check that Aether is running, then try again.
						{/if}
					</p>
				</div>
			</div>
			<Button size="sm" variant="outline" onclick={retry}>
				<RefreshCwIcon class="size-3.5" data-icon="inline-start" />
				Try again
			</Button>
		</div>
	</div>
{:else}
	<div
		class="flex min-h-screen w-full items-center justify-center bg-background px-6 py-12"
		role="alert"
		aria-live="polite"
		aria-busy={retrying}
	>
		<div class="w-full max-w-md space-y-8">
			<div class="space-y-4">
				<div
					class="flex size-14 items-center justify-center rounded-2xl border border-border bg-card text-destructive shadow-sm"
					aria-hidden="true"
				>
					<CloudOffIcon class="size-7" />
				</div>
				<div class="space-y-2">
					<p class="text-xs font-semibold tracking-[0.2em] text-muted-foreground uppercase">
						Connection
					</p>
					<h1 class="text-3xl font-semibold tracking-tight">Server unreachable</h1>
					<p class="text-sm leading-relaxed text-muted-foreground">
						Aether’s API did not answer. The process may be stopped, the proxy may be misconfigured,
						or the database may be down.
					</p>
				</div>
			</div>

			<div
				class="space-y-3 rounded-lg border border-border bg-card p-4 text-card-foreground shadow-sm"
			>
				{#if retrying}
					<div class="flex items-center gap-3">
						<span class="relative flex size-2.5 shrink-0" aria-hidden="true">
							<span class="absolute inline-flex size-full animate-ping rounded-full bg-warning/60"
							></span>
							<span class="relative inline-flex size-2.5 rounded-full bg-warning"></span>
						</span>
						<div class="min-w-0 text-sm">
							<p class="font-medium">Reconnecting automatically</p>
							<p class="text-muted-foreground">
								{#if attempt > 0}
									Attempt {attempt} · trying again every few seconds
								{:else}
									Trying again every few seconds
								{/if}
							</p>
						</div>
					</div>
				{:else}
					<p class="text-sm text-muted-foreground">
						Automatic reconnect is paused. Start the server, then try again.
					</p>
				{/if}
			</div>

			<div class="flex flex-wrap gap-2">
				<Button onclick={retry}>
					<RefreshCwIcon class="size-4" data-icon="inline-start" />
					Try again
				</Button>
			</div>
		</div>
	</div>
{/if}
