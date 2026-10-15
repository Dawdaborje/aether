<script lang="ts">
	let {
		plugin,
		name,
		functionName,
		source
	}: {
		plugin: string;
		name: string;
		functionName: string;
		source: string;
	} = $props();
</script>

<div class="font-medium">{plugin}.{name}</div>
<div class="text-xs text-muted-foreground">{functionName} · {source}</div>
