<script lang="ts">
	import { loadFailure } from '$lib/errors/report';
	import { page } from '$app/stores';
	import { resolve } from '$app/paths';
	import { fetchOrganizations, type OrganizationProfile } from '$lib/org/api';
	import { setContext } from 'svelte';

	let { children, params }: { children: import('svelte').Snippet; params: { db: string } } =
		$props();

	let organizations = $state<OrganizationProfile[] | null>(null);
	let failed = $state<string | null>(null);

	setContext('org-children', {
		get organizations() {
			return organizations;
		},
		get failed() {
			return failed;
		}
	});

	$effect(() => {
		void params.db;
		organizations = null;
		failed = null;
		fetchOrganizations()
			.then((list) => (organizations = list))
			.catch((error) => {
				failed = loadFailure(error, 'Organizations could not be loaded.');
				organizations = [];
			});
	});

	const tabs = $derived([
		{ id: 'list', label: 'List', href: `/desk/organizations/${params.db}/children/list` },
		{ id: 'graph', label: 'Graph', href: `/desk/organizations/${params.db}/children/graph` }
	]);
	const active = $derived($page.url.pathname.includes('/children/graph') ? 'graph' : 'list');
</script>

<div class="flex min-h-0 w-full flex-1 flex-col gap-4">
	<nav class="flex shrink-0 gap-1" aria-label="Child organization views">
		{#each tabs as tab (tab.id)}
			<a
				href={resolve(tab.href)}
				class="rounded-md px-3 py-1.5 text-sm transition-colors {active === tab.id
					? 'bg-muted font-medium text-foreground'
					: 'text-muted-foreground hover:text-foreground'}"
				aria-current={active === tab.id ? 'page' : undefined}
			>
				{tab.label}
			</a>
		{/each}
	</nav>
	{@render children()}
</div>
