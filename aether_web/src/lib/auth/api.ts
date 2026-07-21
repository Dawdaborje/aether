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
};

export type MeResponse = {
	user: AuthUser;
	session: {
		provider?: string | null;
		org_database_id?: string | null;
	};
};

export async function fetchAuthMethods(): Promise<AuthMethods> {
	const res = await fetch('/api/auth/methods', { credentials: 'include' });
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
	const res = await fetch('/api/auth/login', {
		method: 'POST',
		credentials: 'include',
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
	const res = await fetch('/api/auth/me', { credentials: 'include' });
	if (res.status === 401) return null;
	if (!res.ok) return null;
	return res.json();
}

export async function logout(): Promise<void> {
	await fetch('/api/auth/logout', { method: 'POST', credentials: 'include' });
}

export function oauthStartUrl(provider: string): string {
	const redirect = `${window.location.origin}/api/auth/oauth/${provider}/callback`;
	return `/api/auth/oauth/${provider}/start?redirect_uri=${encodeURIComponent(redirect)}`;
}
