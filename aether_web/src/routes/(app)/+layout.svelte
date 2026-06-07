<script lang="ts">
	let { children } = $props();
</script>

<div class="flex flex-col">
	<div class="fixed top-0 z-50">hello</div>

	<div class="flex-1">
		{@render children()}
	</div>
</div>
