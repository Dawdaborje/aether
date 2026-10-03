<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { authSession } from '$lib/auth/session.svelte';
	import { orgStore } from '$lib/org/orgStore.svelte';
	import SunIcon from '@lucide/svelte/icons/sun';
	import MoonIcon from '@lucide/svelte/icons/moon';
	import MonitorIcon from '@lucide/svelte/icons/monitor';
	import { themeStore } from '$lib/theme/themeStore.svelte';

	let { tone = 'light' }: { tone?: 'light' | 'on-primary' } = $props();

	const user = $derived(authSession.me?.user ?? null);
	const name = $derived(user?.display_name || user?.username || user?.email || 'Account');
	const initials = $derived(
		name
			.split(/[\s@._-]+/)
			.filter(Boolean)
			.slice(0, 2)
			.map((part) => part.charAt(0).toUpperCase())
			.join('') || '?'
	);

	async function leaveOrganization() {
		await orgStore.leave();
		if (!orgStore.error) await goto(resolve('/organizations'));
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
					class="flex size-9 cursor-pointer items-center justify-center rounded-full text-xs font-semibold outline-none {tone ===
					'on-primary'
						? 'bg-white/20 text-white hover:bg-white/30'
						: 'bg-primary text-primary-foreground hover:opacity-90'}"
				>
					{initials}
				</button>
			{/snippet}
		</DropdownMenu.Trigger>
		<DropdownMenu.Content align="end" class="w-64">
			<DropdownMenu.Label class="space-y-0.5 font-normal">
				<p class="text-sm font-medium">{name}</p>
				{#if user.email}<p class="text-xs text-muted-foreground">{user.email}</p>{/if}
				{#if authSession.isDeveloper}
					<p class="pt-1 text-[10px] font-semibold tracking-widest text-primary uppercase">
						Developer
					</p>
				{/if}
			</DropdownMenu.Label>
			<DropdownMenu.Separator />
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
					onValueChange={(v) => themeStore.setUserMode(v === 'system' ? null : (v as 'light' | 'dark'))}
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
