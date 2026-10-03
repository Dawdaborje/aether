import { apiFetch } from '$lib/api/client';

/** Client helpers for /api/auth */

export type AuthMethods = {
	primary_method: string;
	enabled_methods: string[];
	allow_registration: boolean;
	org?: string | null;
};

export type AuthUser = {
	id: string;
	username?: string | null;
	email?: string | null;
	display_name?: string | null;
	is_super_user: boolean;
	/** The account that may use the developer tools (the kernel stores it as a superuser). */
	is_developer: boolean;
};

export type MeResponse = {
	user: AuthUser;
	session: {
		provider?: string | null;
		org_database_id?: string | null;
	};
};

export async function fetchAuthMethods(): Promise<AuthMethods> {
	const res = await apiFetch('/api/auth/methods');
	if (!res.ok) {
		return {
			primary_method: 'local',
			enabled_methods: ['local'],
			allow_registration: false
		};
	}
	return res.json();
}

export async function loginLocal(username: string, password: string): Promise<MeResponse['user']> {
	const res = await apiFetch('/api/auth/login', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ username, password })
	});
	const body = await res.json().catch(() => ({}));
	if (!res.ok) {
		throw new Error(body.error ?? 'Login failed');
	}
	return body.user;
}

export async function fetchMe(): Promise<MeResponse | null> {
	const res = await apiFetch('/api/auth/me');
	if (res.status === 401) return null;
	if (!res.ok) return null;
	return res.json();
}

export async function logout(): Promise<void> {
	await apiFetch('/api/auth/logout', { method: 'POST' });
}

export function oauthStartUrl(provider: string): string {
	const redirect = `${window.location.origin}/api/auth/oauth/${provider}/callback`;
	return `/api/auth/oauth/${provider}/start?redirect_uri=${encodeURIComponent(redirect)}`;
}
