<script lang="ts">
	import ControlPanel from '$lib/components/layout/shell/ControlPanel.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { authSession } from '$lib/auth/session.svelte';
	import { landingPath } from '$lib/auth/landing';
	import { onMount } from 'svelte';

	// Developer tools are for developers (the superuser account) only.
	onMount(() => {
		if (!authSession.isDeveloper) {
			void goto(resolve(landingPath(authSession.me?.user)));
		}
	});

	const tools = [
		{
			name: 'Models',
			description: 'Define the records of each plugin: fields, types and rules. Renaming a field keeps its data.',
			href: '/studio/models'
		},
		{ name: 'Plugins', description: 'Browse the catalog, versions and what each organization has installed.' },
		{ name: 'Audit trail', description: 'Who visited which page and every record they touched.' },
		{ name: 'Schema', description: 'Inspect organization databases and applied migrations.' },
		{ name: 'API explorer', description: 'Try plugin functions and kernel commands against an organization.' }
	];
</script>

{#if authSession.isDeveloper}
	<ControlPanel crumbs={[{ label: 'Developer tools' }]} />
	<div class="w-full space-y-4 px-4 py-4 md:px-6">
		<p class="text-sm text-muted-foreground">
			Tools for building and inspecting the system. Models is ready; the rest are being built.
		</p>

		<ul class="grid gap-4 sm:grid-cols-2">
			{#each tools as tool (tool.name)}
				{#if tool.href}
					<li>
						<a
							href={resolve(tool.href as '/')}
							class="block rounded-md border border-border bg-card p-5 transition hover:border-primary/50 hover:shadow-sm"
						>
							<h2 class="text-sm font-semibold">{tool.name}</h2>
							<p class="mt-2 text-sm text-muted-foreground">{tool.description}</p>
						</a>
					</li>
				{:else}
					<li class="rounded-md border border-dashed border-border bg-card p-5">
						<div class="flex items-center justify-between">
							<h2 class="text-sm font-semibold">{tool.name}</h2>
							<span
								class="rounded-sm bg-muted px-2 py-0.5 text-[10px] font-semibold tracking-widest text-muted-foreground uppercase"
								>Soon</span
							>
						</div>
						<p class="mt-2 text-sm text-muted-foreground">{tool.description}</p>
					</li>
				{/if}
			{/each}
		</ul>
	</div>
{/if}
