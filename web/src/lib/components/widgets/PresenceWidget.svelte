<script lang="ts">
	import { untrack } from 'svelte';
	import type { PageNode } from '$lib/dsl/types';
	import { usePresence } from '$lib/pages/liveTopic.svelte';
	import { getPageState, text } from '$lib/pages/pageContext.svelte';
	import { fit, initial, peopleFor, readPeople, type Person } from '$lib/pages/presence';

	/**
	 * `<presence topic="channel.{channel}" source="people"/>`: who is connected to a topic now, as a row of avatars
	 * that follows people arriving and leaving. The topic must be declared with `presence = true`. Being on the page
	 * counts as being connected. `source` (optional) is a function of the plugin called with `{ users: [ids] }` that
	 * answers `[{ id, name, image? }]`; without it people are shown by the end of their id.
	 */
	let { node }: { node: PageNode } = $props();

	const page = getPageState();
	const presence = usePresence(() => node.topic);
	const source = $derived(text(node.source));
	const max = $derived(Math.min(20, Math.max(1, Number(node.max) || 6)));

	let known = $state(new Map<string, Person>());
	let latest = 0;

	// Ask for the names of people not known yet; people already named are not asked again.
	$effect(() => {
		const users = presence.users;
		const fn = source;
		if (!fn || !page || users.length === 0) return;
		untrack(() => {
			const missing = users.filter((u) => !known.has(u.replace(/^[^:]*:/, '')));
			if (missing.length === 0) return;
			const current = ++latest;
			void page.call(fn, { ...page.args, users: missing }).then((result) => {
				if (current !== latest || !result.ok) return;
				const next = new Map(known);
				for (const [key, person] of readPeople(result.data)) next.set(key, person);
				known = next;
			});
		});
	});

	const people = $derived(peopleFor(presence.users, known));
	const view = $derived(fit(people, max));
</script>

{#if presence.joined}
	<div
		class="flex items-center gap-2 px-3 py-1.5"
		data-part="presence"
		role="group"
		aria-label={text(node.label) ?? 'People here'}
	>
		<ul class="flex -space-x-2">
			{#each view.shown as person (person.id)}
				<li
					class="grid size-8 place-items-center rounded-full border-2 border-background bg-muted text-xs font-medium"
					title={person.name}
					data-part="presence-person"
				>
					{#if person.image}<img
							src={person.image}
							alt=""
							class="size-full rounded-full object-cover"
						/>
					{:else}{initial(person.name)}{/if}
					<span class="sr-only">{person.name}</span>
				</li>
			{/each}
			{#if view.more > 0}
				<li
					class="grid size-8 place-items-center rounded-full border-2 border-background bg-muted text-xs"
				>
					+{view.more}
				</li>
			{/if}
		</ul>
		<span class="text-xs text-muted-foreground" data-part="presence-count">
			{people.length}
			{people.length === 1 ? 'person' : 'people'} here
		</span>
	</div>
{/if}
