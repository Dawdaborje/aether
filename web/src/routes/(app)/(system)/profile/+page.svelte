<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Label } from '$lib/components/ui/label';
	import UserAvatar from '$lib/components/auth/UserAvatar.svelte';
	import {
		clearProfileAvatar,
		updateProfile,
		uploadProfileAvatar
	} from '$lib/auth/api';
	import { authSession } from '$lib/auth/session.svelte';

	const user = $derived(authSession.me?.user ?? null);
	const name = $derived(user?.display_name || user?.username || user?.email || 'Account');

	let displayName = $state('');
	let working = $state(false);
	let photoWorking = $state(false);
	let message = $state<string | null>(null);
	let failed = $state<string | null>(null);
	let photoInput: HTMLInputElement | undefined = $state();
	let photoCacheKey = $state(Date.now());

	$effect(() => {
		if (user) displayName = user.display_name ?? '';
	});

	async function save(event: SubmitEvent) {
		event.preventDefault();
		if (working || !user) return;
		working = true;
		failed = null;
		message = null;
		try {
			const updated = await updateProfile(displayName);
			await authSession.load();
			authSession.applyUser(updated);
			message = 'Profile saved.';
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Could not save.';
		} finally {
			working = false;
		}
	}

	async function onPhotoSelected(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file || photoWorking) return;
		photoWorking = true;
		failed = null;
		message = null;
		try {
			const updated = await uploadProfileAvatar(file);
			await authSession.load();
			authSession.applyUser(updated);
			photoCacheKey = Date.now();
			message = 'Photo updated.';
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Could not upload the photo.';
		} finally {
			photoWorking = false;
		}
	}

	async function removePhoto() {
		if (photoWorking) return;
		photoWorking = true;
		failed = null;
		message = null;
		try {
			const updated = await clearProfileAvatar();
			await authSession.load();
			authSession.applyUser(updated);
			photoCacheKey = Date.now();
			message = 'Photo removed.';
		} catch (error) {
			failed = error instanceof Error ? error.message : 'Could not remove the photo.';
		} finally {
			photoWorking = false;
		}
	}
</script>

{#if user}
	<div class="mx-auto w-full max-w-3xl space-y-8 p-6 md:p-10">
		<div>
			<h1 class="text-2xl font-semibold tracking-tight">Profile</h1>
			<p class="mt-1 text-sm text-muted-foreground">
				Your account details and profile photo.
			</p>
		</div>

		<div class="grid gap-8 lg:grid-cols-[minmax(0,1fr)_16rem]">
			<form class="space-y-5" onsubmit={save}>
				<div class="space-y-1.5">
					<Label for="profile-display-name">Display name</Label>
					<Input
						id="profile-display-name"
						bind:value={displayName}
						autocomplete="name"
						maxlength={100}
					/>
				</div>
				<div class="space-y-1.5">
					<Label for="profile-username">Username</Label>
					<Input
						id="profile-username"
						value={user.username ?? ''}
						readonly
						class="bg-muted/40"
					/>
				</div>
				<div class="space-y-1.5">
					<Label for="profile-email">Email</Label>
					<Input id="profile-email" value={user.email ?? ''} readonly class="bg-muted/40" />
				</div>

				{#if failed}
					<p class="text-sm text-destructive" role="alert">{failed}</p>
				{/if}
				{#if message}
					<p class="text-sm text-primary" role="status">{message}</p>
				{/if}

				<Button type="submit" disabled={working}>{working ? 'Saving…' : 'Save changes'}</Button>
			</form>

			<aside class="space-y-3 rounded-lg border border-border p-4">
				<p class="text-sm font-medium">Photo</p>
				<p class="text-xs text-muted-foreground">
					Shown on your account menu. Until you upload one, initials are used.
				</p>
				<button
					type="button"
					class="group mx-auto block rounded-full outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-60"
					disabled={photoWorking}
					onclick={() => photoInput?.click()}
					aria-label={user.avatar_url ? 'Change profile photo' : 'Upload profile photo'}
				>
					<span class="relative block">
						<UserAvatar
							{name}
							avatarUrl={user.avatar_url}
							cacheKey={photoCacheKey}
							class="size-28 text-2xl"
							fallbackClass="bg-primary text-primary-foreground text-2xl font-semibold"
						/>
						<span
							class="pointer-events-none absolute inset-0 flex items-end justify-center rounded-full bg-black/45 pb-3 text-xs font-medium text-white opacity-0 transition group-hover:opacity-100"
						>
							{photoWorking ? 'Uploading…' : user.avatar_url ? 'Change' : 'Upload'}
						</span>
					</span>
				</button>
				<input
					bind:this={photoInput}
					type="file"
					accept="image/png,image/jpeg,image/webp,image/gif"
					class="sr-only"
					disabled={photoWorking}
					onchange={onPhotoSelected}
				/>
				<div class="flex flex-col gap-2">
					<Button
						type="button"
						variant="outline"
						size="sm"
						class="w-full"
						disabled={photoWorking}
						onclick={() => photoInput?.click()}
					>
						{photoWorking ? 'Uploading…' : user.avatar_url ? 'Replace photo' : 'Upload photo'}
					</Button>
					{#if user.avatar_url}
						<Button
							type="button"
							variant="ghost"
							size="sm"
							class="w-full"
							disabled={photoWorking}
							onclick={removePhoto}
						>
							Remove photo
						</Button>
					{/if}
				</div>
				<p class="text-xs text-muted-foreground">PNG, JPG, WebP or GIF, up to 2 MB.</p>
			</aside>
		</div>
	</div>
{/if}
