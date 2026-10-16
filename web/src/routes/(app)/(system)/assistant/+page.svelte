<script lang="ts">
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import AiChatPanel from '$lib/components/ai/AiChatPanel.svelte';
</script>

<ControlPanel crumbs={[{ label: 'Assistant' }]} />

<div class="mx-auto h-[calc(100dvh-9rem)] w-full max-w-3xl px-4 py-4 md:px-6">
	<AiChatPanel />
</div>
