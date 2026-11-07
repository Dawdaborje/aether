<script lang="ts">
	/** Neutral first-paint while session / org / theme load — no app chrome. */
</script>

<div
	class="flex min-h-screen w-full flex-col items-center justify-center gap-4 bg-background px-6"
	aria-busy="true"
	role="status"
>
	<p class="text-xs font-semibold tracking-[0.2em] text-muted-foreground uppercase">Aether</p>
	<div class="h-1 w-40 overflow-hidden rounded-full bg-muted">
		<div
			class="h-full w-1/3 animate-[loading_1.1s_ease-in-out_infinite] rounded-full bg-primary/70"
		></div>
	</div>
	<p class="text-sm text-muted-foreground">Preparing your workspace…</p>
</div>
