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
	/** Profile photo URL (`/api/auth/me/avatar`) when one is uploaded. */
	avatar_url?: string | null;
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
	/** Roles in the current organization (empty when none selected). */
	roles?: string[];
	/** Owner of the current organization. */
	is_owner?: boolean;
	/** May list/manage members in the current organization (or everywhere if developer). */
	can_manage_users?: boolean;
	/** Owner or org_admin of the current organization (or developer): Settings and org config. */
	can_manage_org?: boolean;
	/** Developer of the current organization: unrestricted, or made one of this organization. */
	is_org_developer?: boolean;
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

/** The server could not answer (database down, timeout, network): not the same as "logged out". */
export class ServerUnavailableError extends Error {
	constructor(message = 'The server is unavailable') {
		super(message);
		this.name = 'ServerUnavailableError';
	}
}

function isMeResponse(value: unknown): value is MeResponse {
	if (!value || typeof value !== 'object') return false;
	const user = (value as MeResponse).user;
	return Boolean(user && typeof user === 'object' && typeof user.id === 'string');
}

/**
 * The signed-in user, or `null` when there is no valid session (401, or 403 for
 * a deactivated account). Throws {@link ServerUnavailableError} when the server
 * cannot answer, so an outage is never mistaken for being logged out.
 */
export async function fetchMe(): Promise<MeResponse | null> {
	let res: Response;
	try {
		res = await apiFetch('/api/auth/me');
	} catch {
		throw new ServerUnavailableError();
	}
	if (res.status === 401 || res.status === 403) return null;
	if (!res.ok) throw new ServerUnavailableError(`The server answered ${res.status}`);
	let body: unknown;
	try {
		body = await res.json();
	} catch {
		throw new ServerUnavailableError('The server returned a non-JSON identity response');
	}
	if (!isMeResponse(body)) {
		throw new ServerUnavailableError('The server returned an invalid identity response');
	}
	return body;
}

export async function logout(): Promise<void> {
	await apiFetch('/api/auth/logout', { method: 'POST' });
}

async function readError(res: Response, fallback: string): Promise<string> {
	const body = await res.json().catch(() => ({}));
	return typeof body.error === 'string' ? body.error : fallback;
}

export async function updateProfile(displayName: string): Promise<AuthUser> {
	const res = await apiFetch('/api/auth/me', {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ display_name: displayName })
	});
	if (!res.ok) {
		throw new Error(await readError(res, `Profile could not be saved (${res.status}).`));
	}
	const body = (await res.json()) as { user: AuthUser };
	return body.user;
}

const MAX_AVATAR_BYTES = 2_000_000;

function avatarFilename(file: File): string {
	const named = file.name?.trim();
	if (named && /\.(png|jpe?g|jfif|webp|gif)$/i.test(named)) return named;
	const mime = (file.type || '').toLowerCase();
	const ext =
		mime === 'image/png'
			? 'png'
			: mime === 'image/webp'
				? 'webp'
				: mime === 'image/gif'
					? 'gif'
					: mime === 'image/jpeg' || mime === 'image/jpg'
						? 'jpg'
						: '';
	if (!ext) return named || 'avatar.jpg';
	return `avatar.${ext}`;
}

export async function uploadProfileAvatar(file: File): Promise<AuthUser> {
	if (file.size <= 0 || file.size > MAX_AVATAR_BYTES) {
		throw new Error('Photo must be between 1 byte and 2 MB.');
	}
	const data = await new Promise<string>((resolve, reject) => {
		const reader = new FileReader();
		reader.onload = () => resolve(String(reader.result ?? ''));
		reader.onerror = () => reject(new Error('Could not read the photo file.'));
		reader.readAsDataURL(file);
	});
	const res = await apiFetch('/api/auth/me/avatar', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ filename: avatarFilename(file), data })
	});
	if (!res.ok) {
		if (res.status === 413) {
			throw new Error('Photo is too large to upload (max 2 MB).');
		}
		throw new Error(await readError(res, `Photo could not be uploaded (${res.status}).`));
	}
	const body = (await res.json()) as { user: AuthUser };
	return body.user;
}

export async function clearProfileAvatar(): Promise<AuthUser> {
	const res = await apiFetch('/api/auth/me/avatar', { method: 'DELETE' });
	if (!res.ok) {
		throw new Error(await readError(res, `Photo could not be removed (${res.status}).`));
	}
	const body = (await res.json()) as { user: AuthUser };
	return body.user;
}

export async function consumeMagicLink(
	token: string,
	newPassword?: string
): Promise<{ ok: boolean; purpose?: string; path?: string }> {
	const res = await apiFetch('/api/auth/magic-link/consume', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({
			token,
			new_password: newPassword
		})
	});
	const body = await res.json().catch(() => ({}));
	if (!res.ok) {
		throw new Error(typeof body.error === 'string' ? body.error : 'Magic link failed');
	}
	return body;
}

export function oauthStartUrl(provider: string): string {
	const redirect = `${window.location.origin}/api/auth/oauth/${provider}/callback`;
	return `/api/auth/oauth/${provider}/start?redirect_uri=${encodeURIComponent(redirect)}`;
}
