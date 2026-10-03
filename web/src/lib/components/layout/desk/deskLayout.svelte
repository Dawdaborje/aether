<script lang="ts">
	import { deskNav } from '../nav';
	import EnterpriseShell from '../shell/EnterpriseShell.svelte';

	let { children } = $props();
</script>

<!-- The developer desk: the same shell as the default layout, with the desk's navigation. -->
<EnterpriseShell nav={deskNav}>
	{@render children()}
</EnterpriseShell>
