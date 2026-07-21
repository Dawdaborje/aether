<script lang="ts">
	import { resolve } from '$app/paths';
</script>

<div class="flex min-h-screen items-center justify-center px-6">
	<div class="max-w-md space-y-4 text-center">
		<h1 class="text-2xl font-semibold">Registration closed</h1>
		<p class="text-sm text-muted-foreground">
			Self-registration is controlled by
			<code class="text-xs">auth.allow_registration</code> and is off by default.
		</p>
		<a class="text-sm underline" href={resolve('/login')}>Back to sign in</a>
	</div>
</div>
