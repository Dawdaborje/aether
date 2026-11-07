<script lang="ts">
	import RoleDetail from '$lib/components/org/RoleDetail.svelte';

	let { params }: { params: { db: string; role: string } } = $props();

	const roleName = $derived(decodeURIComponent(params.role));
	const listHref = $derived(`/desk/organizations/${params.db}/roles`);
</script>

<div class="px-1 py-2 md:px-0">
	<RoleDetail db={params.db} {roleName} {listHref} />
</div>
