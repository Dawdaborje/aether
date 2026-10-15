<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import UserAvatar from '$lib/components/auth/UserAvatar.svelte';
	import { authSession } from '$lib/auth/session.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import SunIcon from '@lucide/svelte/icons/sun';
	import MoonIcon from '@lucide/svelte/icons/moon';
	import MonitorIcon from '@lucide/svelte/icons/monitor';
	import ClockIcon from '@lucide/svelte/icons/clock';
	import SettingsIcon from '@lucide/svelte/icons/settings';
	import ShieldIcon from '@lucide/svelte/icons/shield';
	import UserIcon from '@lucide/svelte/icons/user';
	import UsersIcon from '@lucide/svelte/icons/users';
	import { themeStore } from '$lib/theme/themeStore.svelte';

	let { tone = 'light' }: { tone?: 'light' | 'on-primary' } = $props();

	const user = $derived(authSession.me?.user ?? null);
	const name = $derived(user?.display_name || user?.username || user?.email || 'Account');

	async function leaveOrganization() {
		await orgStore.leave();
		if (!orgStore.error) await goto(resolve('/desk/organizations'));
	}

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
					<p class="pt-1 text-xs font-semibold tracking-widest text-primary uppercase">
						Developer
					</p>
				{/if}
			</DropdownMenu.Label>
			<DropdownMenu.Separator />
			<DropdownMenu.Item onclick={() => goto(resolve('/profile'))}>
				<UserIcon /> Profile
			</DropdownMenu.Item>
			{#if authSession.canManageUsers && !authSession.isDeveloper}
				<DropdownMenu.Item onclick={() => goto(resolve('/users'))}>
					<UsersIcon /> Users
				</DropdownMenu.Item>
				<DropdownMenu.Item onclick={() => goto(resolve('/roles'))}>
					<ShieldIcon /> Roles
				</DropdownMenu.Item>
			{/if}
			{#if authSession.canManageOrg || authSession.isDeveloper}
				<DropdownMenu.Item onclick={() => goto(resolve('/desk/settings'))}>
					<SettingsIcon /> Settings
				</DropdownMenu.Item>
			{/if}
			{#if authSession.isOrgDeveloper && !authSession.isDeveloper}
				<DropdownMenu.Item onclick={() => goto(resolve('/schedules'))}>
					<ClockIcon /> Schedules
				</DropdownMenu.Item>
			{/if}
			{#if orgStore.canSwitch}
				<DropdownMenu.Item onclick={() => orgStore.openSwitcher()}>
					Switch organization{orgStore.currentName ? ` (${orgStore.currentName})` : ''}
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
						<SunIcon /> Light
					</DropdownMenu.RadioItem>
					<DropdownMenu.RadioItem value="dark" class="cursor-pointer">
						<MoonIcon /> Dark
					</DropdownMenu.RadioItem>
					<DropdownMenu.RadioItem value="system" class="cursor-pointer">
						<MonitorIcon /> System
					</DropdownMenu.RadioItem>
				</DropdownMenu.RadioGroup>
			</DropdownMenu.Group>
			{#if orgStore.canLeave}
				<DropdownMenu.Item onclick={leaveOrganization}>
					Leave organization{orgStore.currentName ? ` (${orgStore.currentName})` : ''}
				</DropdownMenu.Item>
			{/if}
			<DropdownMenu.Separator />
			<DropdownMenu.Item onclick={signOut}>Sign out</DropdownMenu.Item>
		</DropdownMenu.Content>
	</DropdownMenu.Root>
{/if}
