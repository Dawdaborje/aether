<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import UserAvatar from '$lib/components/auth/UserAvatar.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { appsStore } from '$lib/apps/appsStore.svelte';
	import { iconRef } from '$lib/icons';
	import AppIcon from '$lib/icons/AppIcon.svelte';
	import { navHref } from '../nav';
	import { themeStore } from '$lib/theme/themeStore.svelte';

	let { tone = 'light' }: { tone?: 'light' | 'on-primary' } = $props();

	const user = $derived(authSession.me?.user ?? null);
	const name = $derived(user?.display_name || user?.username || user?.email || 'Account');

	async function signOut() {
		await authSession.signOut();
		await goto(resolve('/login'));
	}
</script>

{#if user}
	<DropdownMenu.Root>
		<DropdownMenu.Trigger>
			{#snippet child({ props })}
				<button
					{...props}
					type="button"
					aria-label="Account menu"
					class="flex size-9 cursor-pointer items-center justify-center rounded-full outline-none {tone ===
					'on-primary'
						? 'hover:ring-2 hover:ring-region-hover'
						: 'hover:opacity-90'}"
				>
					<UserAvatar
						{name}
						avatarUrl={user.avatar_url}
						class="size-9 text-xs font-semibold"
						fallbackClass={tone === 'on-primary'
							? 'bg-region-hover text-region-hover-foreground'
							: 'bg-primary text-primary-foreground'}
					/>
				</button>
			{/snippet}
		</DropdownMenu.Trigger>
		<DropdownMenu.Content align="end" class="w-64">
			<DropdownMenu.Label class="space-y-0.5 font-normal">
				<p class="text-sm font-medium">{name}</p>
				{#if user.email}<p class="text-xs text-muted-foreground">{user.email}</p>{/if}
				{#if authSession.isDeveloper}
					<p class="pt-1 text-xs font-semibold tracking-widest text-primary uppercase">Developer</p>
				{/if}
			</DropdownMenu.Label>
			<DropdownMenu.Separator />
			<DropdownMenu.Item onclick={() => goto(resolve('/profile'))}>
				<AppIcon name="user" /> Profile
			</DropdownMenu.Item>
			{#if authSession.canManageUsers && !authSession.isDeveloper}
				<DropdownMenu.Item onclick={() => goto(resolve('/users'))}>
					<AppIcon name="users" /> Users
				</DropdownMenu.Item>
				<DropdownMenu.Item onclick={() => goto(resolve('/roles'))}>
					<AppIcon name="shield" /> Roles
				</DropdownMenu.Item>
			{/if}
			{#if authSession.canManageOrg || authSession.isDeveloper}
				<DropdownMenu.Item onclick={() => goto(resolve('/desk/settings'))}>
					<AppIcon name="settings" /> Settings
				</DropdownMenu.Item>
			{/if}
			{#each appsStore.navLinks as link (link.plugin)}
				{@const LinkIcon = iconRef(link.icon)}
				<DropdownMenu.Item onclick={() => goto(navHref(link.href))}>
					{#if LinkIcon}<AppIcon name={LinkIcon} />{/if}
					{link.label}
				</DropdownMenu.Item>
			{/each}
			{#if authSession.isOrgDeveloper && !authSession.isDeveloper}
				<DropdownMenu.Item onclick={() => goto(resolve('/schedules'))}>
					<AppIcon name="clock" /> Schedules
				</DropdownMenu.Item>
			{/if}
			<DropdownMenu.Separator />
			<DropdownMenu.Group>
				<DropdownMenu.GroupHeading>Appearance</DropdownMenu.GroupHeading>
				<DropdownMenu.RadioGroup
					value={themeStore.userMode ?? 'system'}
					onValueChange={(v) =>
						themeStore.setUserMode(v === 'system' ? null : (v as 'light' | 'dark'))}
				>
					<DropdownMenu.RadioItem value="light" class="cursor-pointer">
						<AppIcon name="sun" /> Light
					</DropdownMenu.RadioItem>
					<DropdownMenu.RadioItem value="dark" class="cursor-pointer">
						<AppIcon name="moon" /> Dark
					</DropdownMenu.RadioItem>
					<DropdownMenu.RadioItem value="system" class="cursor-pointer">
						<AppIcon name="monitor" /> System
					</DropdownMenu.RadioItem>
				</DropdownMenu.RadioGroup>
			</DropdownMenu.Group>
			<DropdownMenu.Separator />
			<DropdownMenu.Item onclick={signOut}>Sign out</DropdownMenu.Item>
		</DropdownMenu.Content>
	</DropdownMenu.Root>
{/if}
