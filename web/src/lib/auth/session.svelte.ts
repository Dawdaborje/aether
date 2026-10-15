import { fetchMe, logout, type AuthUser, type MeResponse } from './api';

type Principal =
	| { status: 'loading' }
	| { status: 'user'; me: MeResponse }
	/** Not logged in. On a public page the API still knows the browser as a visitor. */
	| { status: 'anonymous' };

const AUTH_CHANNEL = 'aether-auth';

/**
 * Tell other tabs/windows of this browser that the session cookie changed
 * (login or logout). They share one `aether_session` cookie, so stale in-memory
 * identity must be dropped. A full reload resets org/apps/notification stores too.
 */
export function announceSessionChanged(): void {
	if (typeof BroadcastChannel === 'undefined') return;
	try {
		const channel = new BroadcastChannel(AUTH_CHANNEL);
		channel.postMessage({ type: 'session-changed' });
		channel.close();
	} catch {
		/* private mode / unsupported */
	}
}

function listenForSessionChanges(onChange: () => void): () => void {
	if (typeof BroadcastChannel === 'undefined') return () => {};
	try {
		const channel = new BroadcastChannel(AUTH_CHANNEL);
		channel.onmessage = (event: MessageEvent) => {
			if (event.data?.type === 'session-changed') onChange();
		};
		return () => channel.close();
	} catch {
		return () => {};
	}
}

/**
 * Who is using the app. Public pages work without a login, so "no user" is a
 * normal state rather than an error; the API decides what an anonymous
 * browser may see.
 *
 * One browser profile has one session cookie for all tabs. Signing in as another
 * user in a second tab replaces that cookie; this tab keeps showing the old
 * user until it reloads auth — which an API outage retry used to trigger
 * surprisingly. Cross-tab announcements keep every tab honest immediately.
 */
class AuthSession {
	principal = $state<Principal>({ status: 'loading' });

	constructor() {
		if (typeof window !== 'undefined') {
			listenForSessionChanges(() => {
				// Cookie changed elsewhere: drop every in-memory store by reloading.
				window.location.reload();
			});
		}
	}

	get isUser(): boolean {
		return this.principal.status === 'user';
	}

	/**
	 * Developers (the superuser account) get the developer tools, which are
	 * built on top of this. Use it to hide or guard anything developer-only.
	 */
	get isDeveloper(): boolean {
		return this.principal.status === 'user' && this.principal.me.user.is_developer;
	}

	/** May open the org Users page (owner, org_admin, user_manager, or developer). */
	get canManageUsers(): boolean {
		return this.principal.status === 'user' && this.principal.me.can_manage_users === true;
	}

	/** May open Settings for the current org (owner, org_admin, or developer). */
	get canManageOrg(): boolean {
		return this.principal.status === 'user' && this.principal.me.can_manage_org === true;
	}

	/**
	 * Developer of the current organization: an unrestricted developer, or a member an
	 * unrestricted developer made a developer of this organization. Opens Schedules.
	 */
	get isOrgDeveloper(): boolean {
		return this.principal.status === 'user' && this.principal.me.is_org_developer === true;
	}

	get isOwner(): boolean {
		return this.principal.status === 'user' && this.principal.me.is_owner === true;
	}

	get roles(): string[] {
		return this.principal.status === 'user' ? (this.principal.me.roles ?? []) : [];
	}

	get isLoading(): boolean {
		return this.principal.status === 'loading';
	}

	get me(): MeResponse | null {
		return this.principal.status === 'user' ? this.principal.me : null;
	}

	/** End the session on the server and forget it here. */
	async signOut(): Promise<void> {
		await logout();
		this.principal = { status: 'anonymous' };
		announceSessionChanged();
	}

	async load(): Promise<void> {
		const me = await fetchMe();
		this.principal = me ? { status: 'user', me } : { status: 'anonymous' };
	}

	/** Keep profile fields from a write when a follow-up `/me` is still the old session. */
	applyUser(user: AuthUser): void {
		if (this.principal.status !== 'user') return;
		this.principal = {
			status: 'user',
			me: {
				...this.principal.me,
				user: { ...this.principal.me.user, ...user }
			}
		};
	}
}

export const authSession = new AuthSession();
