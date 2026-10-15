/** Where to send someone after login, or when the page they asked for does not exist. */

import { themeStore } from '$lib/theme/themeStore.svelte';

/** The slice of the user object that decides where they land. */
export type LandingUser = { is_developer: boolean };

/**
 * A developer lands on the desk; everyone else on the app launcher
 * (empty until plugins are installed for their organization).
 */
export function landingPath(user: LandingUser | null | undefined): '/desk/organizations' | '/apps' {
	return user?.is_developer ? '/desk/organizations' : '/apps';
}

/**
 * The organization's home page: the theme's `home.route` when it names one, else the
 * app launcher. `/` itself is never a home: that is the page that sends people here.
 */
export function homePath(): string {
	const route = themeStore.theme.shell?.home?.route;
	return route && route !== '/' ? route : '/apps';
}

/**
 * Send someone to their landing area. Members go to the theme's home page, so the
 * organization's theme is loaded first (right after login it may still be the
 * visitor fallback).
 */
export async function gotoLanding(user: LandingUser | null | undefined): Promise<void> {
	if (user?.is_developer) {
		await goto(resolve('/desk/organizations'));
		return;
	}
	if (themeStore.theme.source !== 'organization') await themeStore.loadFromApi();
	const home = homePath();
	if (home === '/apps') await goto(resolve('/apps'));
	else await gotoPluginPage(home.slice(1));
}

/**
 * Validate a `?next=` value so login can only send people to a path inside
 * this app: it must be an absolute path, and not a protocol-relative URL
 * (`//host`) or contain a backslash. Returns the slug for the page route, or
 * null when `next` is absent or unsafe.
 */
export function nextSlug(next: string | null): string | null {
	if (!next || !next.startsWith('/') || next.startsWith('//') || next.includes('\\')) {
		return null;
	}
	return next.slice(1).replace(/\/+$/, '');
}

import { goto } from '$app/navigation';
import { base, resolve } from '$app/paths';

/**
 * Go to the login page and come back to `next` (an app-relative path such as
 * `/chat/general`) afterwards. The URL is built here, once, because it carries
 * a query string that `resolve()` cannot express.
 */
export function gotoLogin(next: string): Promise<void> {
	// The path comes from resolve(); only a query string is appended.
	// eslint-disable-next-line svelte/no-navigation-without-resolve
	return goto(`${resolve('/login')}?next=${encodeURIComponent(next)}`);
}

/** Go to an app-relative page path for a plugin page (the `[...slug]` route). */
export function gotoPluginPage(slug: string): Promise<void> {
	// `slug` is a validated app path (see nextSlug); `base` is the app's mount point.
	// eslint-disable-next-line svelte/no-navigation-without-resolve
	return goto(slug ? `${base}/${slug}` : base || '/');
}
