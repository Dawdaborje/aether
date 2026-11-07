<script lang="ts">
	import { DetailSkeleton } from '$lib/components/ui/skeleton';
	import LoadFailed from '$lib/components/errors/LoadFailed.svelte';
	import { loadFailure, reportError } from '$lib/errors/report';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { SvelteSet } from 'svelte/reactivity';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import ConfirmDialog from '$lib/components/dialogs/ConfirmDialog.svelte';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import * as Field from '$lib/components/ui/field/index.js';
	import { Input } from '$lib/components/ui/input';
	import { authSession } from '$lib/auth/session.svelte';
	import {
		deleteOrganizationRole,
		fetchOrganizationMembers,
		fetchOrganizationRoles,
		grantMemberRole,
		revokeMemberRole,
		updateOrganizationRole,
		type OrgMember,
		type OrgRole
	} from '$lib/org/api';
	import RoleKernelPermissions from './RoleKernelPermissions.svelte';
	import RoleRecordGrants from './RoleRecordGrants.svelte';
	import RolePermissions from './RolePermissions.svelte';
	import { kindLabel, roleKind } from './roleDisplay';

	let {
		db,
		roleName,
		listHref
	}: {
		db: string;
		roleName: string;
		listHref: string;
	} = $props();

	let role = $state<OrgRole | null>(null);
	let members = $state<OrgMember[]>([]);
	let loading = $state(true);
	let failed = $state<string | null>(null);
	let working = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let editing = $state(false);
	let editLabel = $state('');
	let editDescription = $state('');
	let memberQuery = $state('');
	let grantSelected = new SvelteSet<string>();

	const canManageAdmin = $derived(
		authSession.isDeveloper ||
			authSession.isOwner ||
			(authSession.roles?.includes('org_admin') ?? false)
	);
	const kind = $derived(role ? roleKind(role.name, role.is_system) : 'system');
	const isCustom = $derived(kind === 'custom');
	const canGrantThis = $derived(role?.name !== 'org_admin' || canManageAdmin);
	const canRevokeThis = $derived(canGrantThis);

	const holderIds = $derived(new Set((role?.holders ?? []).map((h) => h.user_id)));
	const candidates = $derived(
		members.filter((member) => {
			if (holderIds.has(member.user_id)) return false;
			const needle = memberQuery.trim().toLowerCase();
			if (!needle) return true;
			return [member.display_name, member.username, member.email, member.user_id]
				.filter(Boolean)
				.join(' ')
				.toLowerCase()
				.includes(needle);
		})
	);

	async function reload() {
		loading = true;
		failed = null;
		error = null;
		try {
			const [nextRoles, nextMembers] = await Promise.all([
				fetchOrganizationRoles(db),
				fetchOrganizationMembers(db)
			]);
			members = nextMembers;
			role = nextRoles.find((row) => row.name === roleName) ?? null;
			if (!role) {
				failed = 'That role was not found in this organization.';
			} else {
				editLabel = role.label;
				editDescription = role.description ?? '';
			}
		} catch (err) {
			failed = loadFailure(err, 'Role could not be loaded.');
			role = null;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void db;
		void roleName;
		void reload();
	});

	/** An action waiting for the person to confirm it in the dialog. */
	type Pending = {
		title: string;
		description: string;
		confirmLabel: string;
		destructive?: boolean;
		action: () => Promise<void>;
	};
	let pending = $state<Pending | null>(null);
	let pendingOpen = $state(false);

	function ask(next: Pending) {
		pending = next;
		pendingOpen = true;
	}

	function confirmPending() {
		const next = pending;
		pendingOpen = false;
		if (next) void run(next.action);
	}

	async function run(action: () => Promise<void>) {
		working = true;
		error = null;
		notice = null;
		try {
			await action();
		} catch (err) {
			error = reportError(err, 'The action failed.');
		} finally {
			working = false;
		}
	}

	function toggleGrant(userId: string, checked: boolean) {
		if (checked) grantSelected.add(userId);
		else grantSelected.delete(userId);
	}

	async function grantSelectedMembers() {
		const ids = [...grantSelected];
		if (!role || ids.length === 0 || !canGrantThis) return;
		await run(async () => {
			for (const userId of ids) {
				await grantMemberRole(db, userId, role!.name);
			}
			grantSelected.clear();
			memberQuery = '';
			await reload();
			notice = ids.length === 1 ? 'Granted to 1 member.' : `Granted to ${ids.length} members.`;
		});
	}

	function memberLabel(member: OrgMember): string {
		return member.display_name || member.username || member.email || member.user_id;
	}
</script>

{#if loading && !role}
	<DetailSkeleton fields={4} label="Loading role…" />
{:else if failed && !role}
	<LoadFailed message={failed} onRetry={reload}>
		<Button size="sm" variant="outline" href={resolve(listHref)}>Back to roles</Button>
	</LoadFailed>
{:else if role}
	<div class="space-y-6">
		<p class="text-sm">
			<a class="text-muted-foreground hover:text-foreground" href={resolve(listHref)}>← Roles</a>
		</p>

		<header class="space-y-2">
			<div class="flex flex-wrap items-center gap-2">
				<h1 class="text-xl font-semibold tracking-tight">{role.label}</h1>
				<Badge variant={isCustom ? 'outline' : 'secondary'}>{kindLabel(kind)}</Badge>
			</div>
			<p class="font-mono text-sm text-muted-foreground">{role.name}</p>
			{#if role.description && !editing}
				<p class="text-sm text-muted-foreground">{role.description}</p>
			{/if}
		</header>

		{#if error}
			<p class="text-sm text-destructive" role="alert">{error}</p>
		{/if}
		{#if notice}
			<p class="text-sm text-muted-foreground" role="status">{notice}</p>
		{/if}

		{#if isCustom}
			<section class="max-w-xl space-y-3 rounded-md border border-border p-4">
				{#if editing}
					<label class="block space-y-1 text-sm">
						<span class="text-muted-foreground">Label</span>
						<Input bind:value={editLabel} disabled={working} />
					</label>
					<label class="block space-y-1 text-sm">
						<span class="text-muted-foreground">Description</span>
						<Input bind:value={editDescription} disabled={working} />
					</label>
					<div class="flex flex-wrap gap-2">
						<Button
							size="sm"
							disabled={working || !editLabel.trim()}
							onclick={() =>
								run(async () => {
									await updateOrganizationRole(db, role.name, {
										label: editLabel.trim(),
										description: editDescription.trim(),
										set: ['description']
									});
									editing = false;
									await reload();
									notice = 'Role updated.';
								})}
						>
							Save
						</Button>
						<Button size="sm" variant="ghost" disabled={working} onclick={() => (editing = false)}>
							Cancel
						</Button>
					</div>
				{:else}
					<div class="flex flex-wrap gap-2">
						<Button size="sm" variant="outline" disabled={working} onclick={() => (editing = true)}>
							Edit
						</Button>
						<Button
							size="sm"
							variant="destructive"
							disabled={working}
							onclick={() => {
								const name = role.name;
								ask({
									title: `Delete role “${role.label}”?`,
									description: 'Everyone who holds it will lose it.',
									confirmLabel: 'Delete role',
									destructive: true,
									action: async () => {
										await deleteOrganizationRole(db, name);
										await goto(resolve(listHref));
									}
								});
							}}
						>
							Delete role
						</Button>
					</div>
				{/if}
			</section>
		{/if}

		<RoleKernelPermissions
			{db}
			roleName={role.name}
			editable={roleKind(role.name, role.is_system) === 'custom' && authSession.canManageRoles}
		/>

		{#if roleKind(role.name, role.is_system) === 'custom'}
			<RoleRecordGrants
				{db}
				roleName={role.name}
				editable={authSession.canManageRoles && authSession.canManageOrg}
			/>
		{/if}

		<RolePermissions {db} roleName={role.name} />

		<section class="space-y-3">
			<h2 class="text-base font-medium tracking-tight">Holders</h2>
			{#if (role.holders ?? []).length === 0}
				<p class="text-sm text-muted-foreground">Nobody holds this role yet.</p>
			{:else}
				<ul class="divide-y divide-border rounded-md border border-border">
					{#each role.holders as holder (holder.user_id)}
						<li class="flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-sm">
							<span>
								<span class="font-medium">{holder.display_name}</span>
								<span class="ml-2 font-mono text-xs text-muted-foreground">{holder.user_id}</span>
							</span>
							{#if canRevokeThis}
								<Button
									size="sm"
									variant="ghost"
									disabled={working}
									onclick={() =>
										run(async () => {
											await revokeMemberRole(db, holder.user_id, role.name);
											await reload();
											notice = `Revoked from ${holder.display_name}.`;
										})}
								>
									Revoke
								</Button>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		{#if canGrantThis}
			<section class="max-w-xl space-y-3">
				<Field.Set>
					<Field.Legend variant="label">Grant to members</Field.Legend>
					<Field.Description>Search members who do not already hold this role.</Field.Description>
					<div class="space-y-3 pt-2">
						<Input
							type="search"
							placeholder="Filter members"
							aria-label="Filter members"
							bind:value={memberQuery}
							disabled={working}
						/>
						{#if candidates.length === 0}
							<p class="text-sm text-muted-foreground">
								{members.length === holderIds.size
									? 'Every member already has this role.'
									: `Nothing matches “${memberQuery.trim()}”.`}
							</p>
						{:else}
							<Field.Group class="max-h-64 gap-2 overflow-y-auto pr-1">
								{#each candidates as member (member.user_id)}
									<Field.Field
										orientation="horizontal"
										class="items-start rounded-sm border border-border/60 px-2 py-2"
									>
										<Checkbox
											id={`grant-member-${member.user_id}`}
											checked={grantSelected.has(member.user_id)}
											onCheckedChange={(checked) => toggleGrant(member.user_id, checked === true)}
											disabled={working}
										/>
										<Field.Label
											for={`grant-member-${member.user_id}`}
											class="min-w-0 flex-1 font-normal"
										>
											<span class="font-medium">{memberLabel(member)}</span>
											{#if member.email}
												<span class="ml-2 text-xs text-muted-foreground">{member.email}</span>
											{/if}
										</Field.Label>
									</Field.Field>
								{/each}
							</Field.Group>
						{/if}
						<Button
							size="sm"
							disabled={working || grantSelected.size === 0}
							onclick={() => void grantSelectedMembers()}
						>
							Grant {grantSelected.size > 0 ? `(${grantSelected.size})` : 'selected'}
						</Button>
					</div>
				</Field.Set>
			</section>
		{:else}
			<p class="text-sm text-muted-foreground">
				Only an administrator or developer may grant or revoke the administrator role.
			</p>
		{/if}
	</div>
{/if}

<ConfirmDialog
	bind:open={pendingOpen}
	title={pending?.title ?? ''}
	description={pending?.description}
	confirmLabel={pending?.confirmLabel}
	destructive={pending?.destructive ?? false}
	busy={working}
	onConfirm={confirmPending}
/>
