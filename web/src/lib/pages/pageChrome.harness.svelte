<script lang="ts">
	import { PageChrome } from './pageChrome.svelte';

	const chrome = new PageChrome();
	const groupByOptions = $derived([{ field: 'department', label: 'Department' }]);

	// Mirrors ListViewWidget: publish chrome, and register the page model for live refresh.
	// The cleanup used to clear `groupByOptions` on every re-run.
	let runs = 0;
	$effect(() => {
		runs += 1;
		if (runs > 8) throw new Error(`chrome effect looped ${runs} times`);
		chrome.model = 'employee';
		chrome.searchable = true;
		chrome.groupByOptions = groupByOptions;
		const unregister = chrome.registerRefreshModel('employee');
		return () => {
			unregister();
			chrome.groupByOptions = [];
		};
	});

	// PageShell watches these to subscribe to data.changed.
	$effect(() => {
		void chrome.model;
		void chrome.refreshModels;
	});
</script>

<p data-testid="models">{chrome.refreshModels.join(',')}</p>
<p data-testid="groups">{chrome.groupByOptions.map((option) => option.field).join(',')}</p>
