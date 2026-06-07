<script lang="ts">
	import AppSidebar from '$lib/components/pages/layout/default/system/defaultNavbar.svelte';

	import * as Sidebar from '$lib/components/ui/sidebar/index.js';
	import type { NavLink } from '$lib/types/layout';

	let { children, navData }: { children: () => any; navData: NavLink[] } = $props();
	let open = $state(true);
</script>

<Sidebar.Provider bind:open>
	<AppSidebar {...{ navData } as any} />
	<Sidebar.Inset>
		<Sidebar.Trigger class="-ms-1" />
		<div class="flex flex-1 flex-col gap-4 p-4">
			{@render children()}
		</div>
	</Sidebar.Inset>
</Sidebar.Provider>
