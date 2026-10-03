import { fetchMe, logout, type MeResponse } from './api';

type Principal =
	| { status: 'loading' }
	| { status: 'user'; me: MeResponse }
	/** Not logged in. On a public page the API still knows the browser as a visitor. */
	| { status: 'anonymous' };

/**
 * Who is using the app. Public pages work without a login, so "no user" is a
 * normal state rather than an error; the API decides what an anonymous
 * browser may see.
 */
class AuthSession {
	principal = $state<Principal>({ status: 'loading' });

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
	}

	async load(): Promise<void> {
		const me = await fetchMe();
		this.principal = me ? { status: 'user', me } : { status: 'anonymous' };
	}
}

export const authSession = new AuthSession();
