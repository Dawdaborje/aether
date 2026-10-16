<script lang="ts">
	import { reportError } from '$lib/errors/report';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import { authSession } from '$lib/auth/session.svelte';
	import {
		fetchOrganization,
		fetchOrganizationMembers,
		fetchOrganizationRoles,
		fetchPlatformUsers,
		grantMemberRole,
		removeOrganizationMember,
		revokeMemberRole,
		sendMemberPasswordReset,
		setMemberActive,
		setMemberDeveloper,
		setMemberPassword,
		type OrganizationProfile,
		type OrgMember,
		type OrgRole,
		type PlatformUser
	} from '$lib/org/api';
	import RoleChip from './RoleChip.svelte';
	import RoleGrantPicker from './RoleGrantPicker.svelte';

	let {
		db,
		userId,
		listHref
	}: {
		db: string;
		userId: string;
		/** Where to return after remove, or when the member is missing. */
		listHref: string;
	} = $props();

	let member = $state<OrgMember | null>(null);
	let platform = $state<PlatformUser | null>(null);
	let organization = $state<OrganizationProfile | null>(null);
	let loading = $state(true);
	let failed = $state<string | null>(null);
	let tab = $state('overview');
	let roles = $state<OrgRole[]>([]);
	let newPassword = $state('');
	let working = $state(false);
	let error = $state<string | null>(null);
	let notice = $state<string | null>(null);
	let resetUrl = $state<string | null>(null);

	const canManageAdmin = $derived(
		authSession.isDeveloper ||
			authSession.isOwner ||
			(authSession.roles?.includes('org_admin') ?? false)
	);
	const canKickOrPassword = $derived(authSession.isDeveloper || authSession.isOwner);
	const grantableRoles = $derived(
		roles.filter((role) => role.name !== 'org_admin' || canManageAdmin)
	);
	const availableToGrant = $derived(
		grantableRoles.filter((role) => !(member?.roles ?? []).includes(role.name))
	);
	const title = $derived(
		member?.display_name ||
			platform?.display_name ||
			member?.username ||
			platform?.username ||
			member?.email ||
			platform?.email ||
			member?.user_id ||
			'User'
	);

	async function reload() {
		loading = true;
		failed = null;
		error = null;
		try {
			const [nextMembers, nextRoles, org, platformUsers] = await Promise.all([
				fetchOrganizationMembers(db),
				fetchOrganizationRoles(db),
				fetchOrganization(db).catch(() => null),
				authSession.isDeveloper
					? fetchPlatformUsers().catch(() => [] as PlatformUser[])
					: Promise.resolve([] as PlatformUser[])
			]);
			roles = nextRoles;
			organization = org;
			member = nextMembers.find((row) => row.user_id === userId) ?? null;
			platform = platformUsers.find((row) => row.user_id === userId) ?? null;
			if (!member) {
				failed = 'That person is not a member of this organization.';
			}
		} catch (err) {
			failed = reportError(err, 'User could not be loaded.');
			member = null;
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void db;
		void userId;
		void reload();
	});

	async function run(action: () => Promise<void>) {
		if (!member) return;
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

	async function grantRoles(names: string[]) {
		if (!member || names.length === 0) return;
		await run(async () => {
			for (const name of names) {
				await grantMemberRole(db, member!.user_id, name);
			}
			member = {
				...member!,
				roles: [...new Set([...(member!.roles ?? []), ...names])].sort()
			};
			roles = await fetchOrganizationRoles(db);
			notice =
				names.length === 1
					? `Granted ${roles.find((r) => r.name === names[0])?.label ?? names[0]}.`
					: `Granted ${names.length} roles.`;
		});
	}
</script>

{#if loading && !member}
	<p class="text-sm text-muted-foreground">Loading user…</p>
{:else if failed && !member}
	<div class="space-y-3">
		<p class="text-sm text-destructive" role="alert">{failed}</p>
		<Button size="sm" variant="outline" href={resolve(listHref)}>Back to users</Button>
	</div>
{:else if member}
	<div class="flex min-h-0 w-full flex-1 flex-col">
		<header class="shrink-0 border-b border-border px-4 py-5 md:px-6">
			<p class="mb-3 text-sm">
				<a class="text-muted-foreground hover:text-foreground" href={resolve(listHref)}>
					← Users
				</a>
			</p>
			<div class="flex flex-wrap items-start gap-4">
				<span
					class="flex size-16 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-xl font-semibold text-primary ring-1 ring-primary/15"
					aria-hidden="true"
				>
					{title.charAt(0).toUpperCase()}
				</span>
				<div class="min-w-0 flex-1 space-y-1">
					<h1 class="text-2xl font-semibold tracking-tight md:text-3xl">{title}</h1>
					<p class="text-sm text-muted-foreground">
						{#if (member.username || platform?.username) && (member.email || platform?.email)}
							{member.username || platform?.username} · {member.email || platform?.email}
						{:else}
							{member.username ||
								platform?.username ||
								member.email ||
								platform?.email ||
								member.user_id}
						{/if}
					</p>
					<div class="flex flex-wrap gap-1.5 pt-1">
						{#if member.is_owner}
							<Badge>Owner</Badge>
						{/if}
						{#if platform?.is_developer}
							<Badge>Developer</Badge>
						{/if}
						{#if member.is_active === false}
							<Badge variant="secondary">Inactive in org</Badge>
						{:else}
							<Badge variant="outline">Active in org</Badge>
						{/if}
						{#if platform && !platform.is_active}
							<Badge variant="secondary">Account inactive</Badge>
						{/if}
					</div>
				</div>
			</div>
		</header>

		<div class="min-h-0 flex-1 px-4 py-5 md:px-6">
			{#if error}
				<p class="mb-4 text-sm text-destructive" role="alert">{error}</p>
			{/if}
			{#if notice}
				<p class="mb-4 text-sm text-muted-foreground" role="status">{notice}</p>
			{/if}

			<Tabs.Root bind:value={tab} class="flex w-full flex-col gap-5">
				<Tabs.List variant="line" class="w-full justify-start rounded-none border-b border-border">
					<Tabs.Trigger value="overview">Overview</Tabs.Trigger>
					<Tabs.Trigger value="roles">Roles</Tabs.Trigger>
					<Tabs.Trigger value="password">Password</Tabs.Trigger>
				</Tabs.List>

				<Tabs.Content value="overview" class="space-y-6">
					<div class="grid gap-6 xl:grid-cols-2">
						<section class="space-y-3">
							<h2 class="text-sm font-medium tracking-tight">Profile</h2>
							<dl class="grid gap-4 rounded-lg border border-border p-4 text-sm sm:grid-cols-2">
								<div>
									<dt class="text-muted-foreground">Display name</dt>
									<dd class="mt-0.5 font-medium">
										{member.display_name || platform?.display_name || '—'}
									</dd>
								</div>
								<div>
									<dt class="text-muted-foreground">Username</dt>
									<dd class="mt-0.5 font-medium">
										{member.username || platform?.username || '—'}
									</dd>
								</div>
								<div>
									<dt class="text-muted-foreground">Email</dt>
									<dd class="mt-0.5 font-medium">{member.email || platform?.email || '—'}</dd>
								</div>
								<div>
									<dt class="text-muted-foreground">User id</dt>
									<dd class="mt-0.5 font-mono text-xs break-all">{member.user_id}</dd>
								</div>
							</dl>
						</section>

						<section class="space-y-3">
							<h2 class="text-sm font-medium tracking-tight">Organization</h2>
							<dl class="grid gap-4 rounded-lg border border-border p-4 text-sm sm:grid-cols-2">
								<div>
									<dt class="text-muted-foreground">Organization</dt>
									<dd class="mt-0.5 font-medium">{organization?.name || db}</dd>
								</div>
								<div>
									<dt class="text-muted-foreground">Database</dt>
									<dd class="mt-0.5 font-mono text-xs">{db}</dd>
								</div>
								<div>
									<dt class="text-muted-foreground">Joined</dt>
									<dd class="mt-0.5">{member.date_joined || '—'}</dd>
								</div>
								<div>
									<dt class="text-muted-foreground">Membership</dt>
									<dd class="mt-0.5">
										{#if member.is_owner}
											Owner
										{:else if member.is_active === false}
											Inactive member
										{:else}
											Active member
										{/if}
									</dd>
								</div>
								<div class="sm:col-span-2">
									<dt class="text-muted-foreground">Roles in this organization</dt>
									<dd class="mt-1.5 flex flex-wrap gap-1.5">
										{#if (member.roles ?? []).length === 0}
											<span class="text-muted-foreground">No roles assigned</span>
										{:else}
											{#each member.roles ?? [] as role (role)}
												<span class="rounded-sm bg-muted px-1.5 py-0.5 font-mono text-xs"
													>{role}</span
												>
											{/each}
										{/if}
									</dd>
								</div>
							</dl>
						</section>

						{#if platform}
							<section class="space-y-3 xl:col-span-2">
								<h2 class="text-sm font-medium tracking-tight">Platform account</h2>
								<dl
									class="grid gap-4 rounded-lg border border-border p-4 text-sm sm:grid-cols-2 lg:grid-cols-4"
								>
									<div>
										<dt class="text-muted-foreground">Account status</dt>
										<dd class="mt-0.5">{platform.is_active ? 'Active' : 'Inactive'}</dd>
									</div>
									<div>
										<dt class="text-muted-foreground">Developer</dt>
										<dd class="mt-0.5">{platform.is_developer ? 'Yes' : 'No'}</dd>
									</div>
									<div class="sm:col-span-2 lg:col-span-2">
										<dt class="text-muted-foreground">Organizations</dt>
										<dd class="mt-1.5 flex flex-wrap gap-1.5">
											{#if platform.organizations.length === 0}
												<span class="text-muted-foreground">None</span>
											{:else}
												{#each platform.organizations as org (org)}
													<span class="rounded-sm bg-muted px-1.5 py-0.5 font-mono text-xs"
														>{org}</span
													>
												{/each}
											{/if}
										</dd>
									</div>
								</dl>
							</section>
						{/if}
					</div>

					{#if authSession.isDeveloper}
						<section class="space-y-3">
							<h2 class="text-sm font-medium tracking-tight">Developer access</h2>
							<p class="text-sm text-muted-foreground">
								{member.is_developer
									? `A developer of ${organization?.name || db}: they can see and run its schedules.`
									: `Not a developer here. A developer of ${organization?.name || db} can see and run its schedules, and nothing in other organizations.`}
							</p>
							<Button
								size="sm"
								variant={member.is_developer ? 'outline' : 'default'}
								disabled={working}
								onclick={() => {
									const target = member;
									if (!target) return;
									void run(async () => {
										const on = !target.is_developer;
										await setMemberDeveloper(db, target.user_id, on);
										member = { ...target, is_developer: on };
										notice = on
											? 'Now a developer of this organization.'
											: 'No longer a developer of this organization.';
									});
								}}
							>
								{member.is_developer ? 'Remove developer access' : 'Make developer of this organization'}
							</Button>
						</section>
					{/if}

					<section class="space-y-3">
						<h2 class="text-sm font-medium tracking-tight">Actions</h2>
						<div class="flex flex-wrap gap-2">
							{#if member.is_active === false}
								<Button
									size="sm"
									disabled={working || member.is_owner}
									onclick={() =>
										run(async () => {
											const members = await setMemberActive(db, member.user_id, true);
											member = members.find((m) => m.user_id === member.user_id) ?? member;
											notice = 'Member reactivated in this organization.';
										})}
								>
									Reactivate in organization
								</Button>
							{:else}
								<Button
									size="sm"
									variant="outline"
									disabled={working || member.is_owner}
									onclick={() => {
										if (
											!confirm(
												'Deactivate this person in the organization? They will not be able to use it until reactivated.'
											)
										)
											return;
										void run(async () => {
											const members = await setMemberActive(db, member.user_id, false);
											member = members.find((m) => m.user_id === member.user_id) ?? {
												...member,
												is_active: false
											};
											notice = 'Member deactivated in this organization.';
										});
									}}
								>
									Deactivate in organization
								</Button>
							{/if}
							{#if canKickOrPassword && !member.is_owner}
								<Button
									size="sm"
									variant="destructive"
									disabled={working}
									onclick={() => {
										if (
											!confirm(
												'Remove this person from the organization? The membership is kept as archived and can be restored by adding them again.'
											)
										)
											return;
										void run(async () => {
											await removeOrganizationMember(db, member.user_id);
											await goto(resolve(listHref));
										});
									}}
								>
									Remove from organization
								</Button>
							{/if}
						</div>
					</section>
				</Tabs.Content>

				<Tabs.Content value="roles" class="space-y-6">
					<p class="text-sm text-muted-foreground">
						Roles control what this person can do in {organization?.name || db}.
					</p>

					<section class="space-y-3">
						<h2 class="text-base font-medium tracking-tight">Current roles</h2>
						<div class="flex flex-wrap items-center gap-2">
							{#each member.roles ?? [] as role (role)}
								<RoleChip
									name={role}
									catalog={roles}
									removable={role !== 'org_admin' || canManageAdmin}
									disabled={working}
									onRevoke={() =>
										run(async () => {
											await revokeMemberRole(db, member.user_id, role);
											member = {
												...member,
												roles: (member.roles ?? []).filter((r) => r !== role)
											};
											roles = await fetchOrganizationRoles(db);
										})}
								/>
							{:else}
								<span class="text-sm text-muted-foreground">No roles yet.</span>
							{/each}
						</div>
					</section>

					<section class="w-full">
						<RoleGrantPicker
							roles={availableToGrant}
							disabled={working}
							{working}
							onGrant={grantRoles}
						/>
					</section>
				</Tabs.Content>

				<Tabs.Content value="password">
					{#if !canKickOrPassword}
						<p class="text-sm text-muted-foreground">
							Only the organization owner or a developer can set or reset passwords.
						</p>
					{:else}
						<div class="max-w-md space-y-3">
							<label class="block space-y-1 text-sm">
								<span class="text-muted-foreground">Set a new password</span>
								<Input type="password" autocomplete="new-password" bind:value={newPassword} />
							</label>
							<Button
								size="sm"
								disabled={working || newPassword.length < 8}
								onclick={() =>
									run(async () => {
										await setMemberPassword(db, member.user_id, newPassword);
										newPassword = '';
										notice = 'Password updated.';
									})}
							>
								Save password
							</Button>
							<div class="border-t border-border pt-3">
								<p class="mb-2 text-sm text-muted-foreground">
									Or email a one-time reset link (multipart plain + HTML).
								</p>
								<Button
									size="sm"
									variant="outline"
									disabled={working}
									onclick={() =>
										run(async () => {
											const result = await sendMemberPasswordReset(db, member.user_id);
											if (result.email_sent) {
												notice = 'Reset link emailed.';
												resetUrl = null;
											} else {
												notice =
													result.email_error ??
													'Email is not configured; copy the link below.';
												resetUrl = result.url ?? null;
											}
										})}
								>
									Send reset link
								</Button>
								{#if resetUrl}
									<label class="mt-2 block space-y-1 text-sm">
										<span class="text-muted-foreground">Reset URL (copy once)</span>
										<input
											class="h-9 w-full rounded-sm border border-input bg-muted px-2 font-mono text-xs"
											readonly
											value={resetUrl}
										/>
									</label>
								{/if}
							</div>
						</div>
					{/if}
				</Tabs.Content>
			</Tabs.Root>
		</div>
	</div>
{/if}
