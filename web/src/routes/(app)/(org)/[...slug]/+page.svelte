<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { buildPage } from '$lib/dsl';
	import PageRenderer from '$lib/components/widgets/PageRenderer.svelte';
	import SpinnerWidget from '$lib/components/widgets/SpinnerWidget.svelte';
	import ErrorPage from '$lib/components/errors/ErrorPage.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { gotoLogin, landingPath } from '$lib/auth/landing';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import { PageState, setPageState } from '$lib/pages/pageContext.svelte';

	let { data, params } = $props();

	// The plugin and route values widgets on this page call functions with.
	const pageState = new PageState();
	setPageState(pageState);
	$effect.pre(() => {
		pageState.plugin = data.body?.plugin ?? '';
		const model = (data.body?.page as { model?: unknown } | undefined)?.model;
		pageState.model = typeof model === 'string' ? model : '';
		pageState.params = data.body?.params ?? {};
	});

	const atRoot = $derived(params.slug === '');

	// 401: a private page, or one that does not exist, for someone who is not
	// logged in: ask them to log in and bring them back. At the root the app
	// shows the 404 page instead, with a Log in button.
	// 404 at the root for a logged-in person: no plugin serves the home page, so
	// land in the system area or the list of apps.
	// 409: the user belongs to several organizations and none is selected.
	const redirecting = $derived(
		(data.status === 401 && !atRoot) || (data.status === 404 && atRoot && authSession.isUser)
	);
	const choosingOrganization = $derived(data.status === 409);

	$effect(() => {
		if (data.status === 401 && !atRoot) {
			void gotoLogin(`/${params.slug}`);
		} else if (data.status === 404 && atRoot && authSession.isUser) {
			void goto(resolve(landingPath(authSession.me?.user)));
		} else if (data.status === 409) {
			void orgStore.requireSelection();
		}
	});

	/** At the root, where "go to the start" would go nowhere, offer the right way in. */
	const rootAction = $derived(
		!atRoot
			? undefined
			: authSession.isDeveloper
				? { label: 'Go to organizations', href: resolve('/organizations') }
				: authSession.isUser
					? { label: 'Open my apps', href: resolve('/apps') }
					: { label: 'Log in', href: resolve('/login') }
	);
	/** A visitor asking for the root is told it does not exist (not that they must log in). */
	const shownStatus = $derived(atRoot && data.status === 401 ? 404 : data.status);

	const built = $derived(data.body ? buildPage(data.body.page) : null);
	// `params` holds the values captured by `{param}` segments of the route
	// (`/chat/{channel}` opened as `/chat/general` gives `{ channel: 'general' }`).
	const pageNode = $derived(
		built && data.body ? { ...built.page, params: data.body.params ?? {} } : null
	);
	/** Any other failure: shown by the active theme's error pages. */
	const error = $derived(data.status !== 200 && !redirecting && !choosingOrganization);
</script>

{#if redirecting || choosingOrganization}
	<div class="p-6">
		<SpinnerWidget
			node={{
				type: 'spinner',
				label: choosingOrganization ? 'Choose an organization…' : 'Redirecting…',
				inline: true
			}}
		/>
	</div>
{:else if error}
	<ErrorPage status={shownStatus} action={rootAction} />
{:else if pageNode}
	{#if built?.warnings.length && import.meta.env.DEV}
		<div
			class="border-b border-amber-500/30 bg-amber-500/5 px-4 py-2 text-xs text-amber-800 dark:text-amber-200"
		>
			DSL warnings: {built.warnings.join('; ')}
		</div>
	{/if}
	<PageRenderer node={pageNode} />
{/if}
