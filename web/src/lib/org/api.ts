import { apiFetch } from '$lib/api/client';

export interface OrganizationProfile {
	db_name: string;
	name: string;
	description?: string | null;
	/** The organization's contact address; older organizations may not have one yet. */
	email?: string | null;
	parent_db_name?: string | null;
	parent_name?: string | null;
	logo_url?: string | null;
	owner_user_id?: string | null;
	owner_login?: string | null;
	member_count: number;
	date_created: string;
	date_updated: string;
}

export interface OrgMember {
	user_id: string;
	username?: string | null;
	email?: string | null;
	display_name?: string | null;
	is_owner: boolean;
	is_active?: boolean;
	date_joined: string;
	roles?: string[];
	/** Developer of this organization only (unrestricted developers are not marked here). */
	is_developer?: boolean;
}

export interface InviteMemberResult {
	members: OrgMember[];
	temporary_password: string;
	username?: string;
	email_sent: boolean;
	email_error?: string | null;
}

export interface PasswordResetResult {
	email_sent: boolean;
	email_error?: string | null;
	url?: string | null;
}

export interface OrgRoleHolder {
	user_id: string;
	display_name: string;
}

export interface OrgRole {
	name: string;
	label: string;
	description?: string | null;
	is_system: boolean;
	holders: OrgRoleHolder[];
	/** The plugin the role belongs to (the part of its name before the dot), when it has one. */
	plugin?: string | null;
	plugin_label?: string | null;
	/** The workspace of that plugin (for example `hrms`, `erp`). */
	workspace?: string | null;
	workspace_label?: string | null;
}

export interface PlatformUser {
	user_id: string;
	username?: string | null;
	email?: string | null;
	display_name?: string | null;
	is_developer: boolean;
	is_active: boolean;
	organizations: string[];
}

export interface OwnershipTransfer {
	from_user_id: string;
	to_user_id: string;
	transferred_by: string;
	from_login?: string | null;
	to_login?: string | null;
	note?: string | null;
	date_created: string;
}

async function readError(res: Response, fallback: string): Promise<string> {
	const body = await res.json().catch(() => ({}));
	return typeof body.error === 'string' ? body.error : fallback;
}

export async function fetchOrganizations(): Promise<OrganizationProfile[]> {
	const res = await apiFetch('/api/ui/organizations');
	if (!res.ok) {
		throw new Error(await readError(res, `Organizations could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { organizations: OrganizationProfile[] };
	return body.organizations ?? [];
}

export async function fetchOrganization(db: string): Promise<OrganizationProfile> {
	const res = await apiFetch(`/api/ui/organizations/${encodeURIComponent(db)}`);
	if (!res.ok) {
		throw new Error(await readError(res, `Organization could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { organization: OrganizationProfile };
	return body.organization;
}

export async function updateOrganization(
	db: string,
	input: { name?: string; email?: string; description?: string; parent?: string; set?: string[] }
): Promise<OrganizationProfile> {
	const res = await apiFetch(`/api/ui/organizations/${encodeURIComponent(db)}`, {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(input)
	});
	if (!res.ok) {
		throw new Error(await readError(res, `Organization could not be saved (${res.status}).`));
	}
	const body = (await res.json()) as { organization: OrganizationProfile };
	return body.organization;
}

export async function uploadOrganizationLogo(
	db: string,
	file: File
): Promise<OrganizationProfile> {
	const data = await new Promise<string>((resolve, reject) => {
		const reader = new FileReader();
		reader.onload = () => resolve(String(reader.result ?? ''));
		reader.onerror = () => reject(new Error('Could not read the logo file.'));
		reader.readAsDataURL(file);
	});
	const res = await apiFetch(`/api/ui/organizations/${encodeURIComponent(db)}/logo`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ filename: file.name, data })
	});
	if (!res.ok) {
		throw new Error(await readError(res, `Logo could not be uploaded (${res.status}).`));
	}
	const body = (await res.json()) as { organization: OrganizationProfile };
	return body.organization;
}

export async function clearOrganizationLogo(db: string): Promise<OrganizationProfile> {
	const res = await apiFetch(`/api/ui/organizations/${encodeURIComponent(db)}/logo`, {
		method: 'DELETE'
	});
	if (!res.ok) {
		throw new Error(await readError(res, `Logo could not be removed (${res.status}).`));
	}
	const body = (await res.json()) as { organization: OrganizationProfile };
	return body.organization;
}

export async function fetchOrganizationMembers(db: string): Promise<OrgMember[]> {
	const res = await apiFetch(`/api/ui/organizations/${encodeURIComponent(db)}/members`);
	if (!res.ok) {
		throw new Error(await readError(res, `Members could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { members: OrgMember[] };
	return body.members ?? [];
}

export async function fetchOrganizationRoles(db: string): Promise<OrgRole[]> {
	const res = await apiFetch(`/api/ui/organizations/${encodeURIComponent(db)}/roles`);
	if (!res.ok) {
		throw new Error(await readError(res, `Roles could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { roles: OrgRole[] };
	return body.roles ?? [];
}

export async function createOrganizationRole(
	db: string,
	input: { name: string; label: string; description?: string }
): Promise<OrgRole> {
	const res = await apiFetch(`/api/ui/organizations/${encodeURIComponent(db)}/roles`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(input)
	});
	if (!res.ok) {
		throw new Error(await readError(res, `Role could not be created (${res.status}).`));
	}
	const body = (await res.json()) as { role: OrgRole };
	return body.role;
}

export async function updateOrganizationRole(
	db: string,
	role: string,
	input: { label?: string; description?: string; set?: string[] }
): Promise<void> {
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/roles/${encodeURIComponent(role)}`,
		{
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(input)
		}
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Role could not be updated (${res.status}).`));
	}
}

export async function deleteOrganizationRole(db: string, role: string): Promise<void> {
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/roles/${encodeURIComponent(role)}`,
		{ method: 'DELETE' }
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Role could not be deleted (${res.status}).`));
	}
}

export async function addOrganizationMember(
	db: string,
	member:
		| { mode: 'new'; username: string; email: string; password: string }
		| { mode: 'existing'; login: string }
		| { mode: 'invite'; email: string; username?: string }
): Promise<OrgMember[] | InviteMemberResult> {
	const res = await apiFetch(`/api/ui/organizations/${encodeURIComponent(db)}/members`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(member)
	});
	if (!res.ok) {
		throw new Error(await readError(res, `Member could not be added (${res.status}).`));
	}
	const body = (await res.json()) as InviteMemberResult & { members: OrgMember[] };
	if (member.mode === 'invite') {
		return {
			members: body.members ?? [],
			temporary_password: body.temporary_password ?? '',
			username: body.username,
			email_sent: body.email_sent === true,
			email_error: body.email_error
		};
	}
	return body.members ?? [];
}

export async function inviteOrganizationMember(
	db: string,
	input: { email: string; username?: string }
): Promise<InviteMemberResult> {
	return (await addOrganizationMember(db, {
		mode: 'invite',
		email: input.email,
		username: input.username
	})) as InviteMemberResult;
}

export async function setMemberActive(
	db: string,
	userId: string,
	isActive: boolean
): Promise<OrgMember[]> {
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/members/${encodeURIComponent(userId)}`,
		{
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ is_active: isActive })
		}
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Member could not be updated (${res.status}).`));
	}
	const body = (await res.json()) as { members: OrgMember[] };
	return body.members ?? [];
}

/** Make a member a developer of this organization (Schedules), or stop that. Unrestricted developers only. */
export async function setMemberDeveloper(
	db: string,
	userId: string,
	isDeveloper: boolean
): Promise<void> {
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/members/${encodeURIComponent(userId)}/developer`,
		{ method: isDeveloper ? 'PUT' : 'DELETE' }
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Developer access could not be changed (${res.status}).`));
	}
}

export async function removeOrganizationMember(db: string, userId: string): Promise<OrgMember[]> {
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/members/${encodeURIComponent(userId)}`,
		{ method: 'DELETE' }
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Member could not be removed (${res.status}).`));
	}
	const body = (await res.json()) as { members: OrgMember[] };
	return body.members ?? [];
}

export async function setMemberPassword(
	db: string,
	userId: string,
	password: string
): Promise<void> {
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/members/${encodeURIComponent(userId)}/password`,
		{
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ password })
		}
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Password could not be set (${res.status}).`));
	}
}

export async function sendMemberPasswordReset(
	db: string,
	userId: string
): Promise<PasswordResetResult> {
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/members/${encodeURIComponent(userId)}/password-reset`,
		{ method: 'POST' }
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Reset link could not be sent (${res.status}).`));
	}
	return (await res.json()) as PasswordResetResult;
}

export async function grantMemberRole(db: string, userId: string, role: string): Promise<void> {
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/members/${encodeURIComponent(userId)}/roles`,
		{
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ role })
		}
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Role could not be granted (${res.status}).`));
	}
}

export async function revokeMemberRole(db: string, userId: string, role: string): Promise<void> {
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/members/${encodeURIComponent(userId)}/roles/${encodeURIComponent(role)}`,
		{ method: 'DELETE' }
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Role could not be revoked (${res.status}).`));
	}
}

export async function fetchPlatformUsers(): Promise<PlatformUser[]> {
	const res = await apiFetch('/api/ui/users');
	if (!res.ok) {
		throw new Error(await readError(res, `Users could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { users: PlatformUser[] };
	return body.users ?? [];
}

export async function createPlatformUser(input: {
	username: string;
	email: string;
	password: string;
}): Promise<PlatformUser[]> {
	const res = await apiFetch('/api/ui/users', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(input)
	});
	if (!res.ok) {
		throw new Error(await readError(res, `User could not be created (${res.status}).`));
	}
	const body = (await res.json()) as { users: PlatformUser[] };
	return body.users ?? [];
}

export async function fetchOwnershipTransfers(db: string): Promise<OwnershipTransfer[]> {
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/ownership-transfers`
	);
	if (!res.ok) {
		throw new Error(await readError(res, `Ownership history could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { transfers: OwnershipTransfer[] };
	return body.transfers ?? [];
}

/** One event of the plugin activity log. */
export interface PluginActivityEvent {
	at: string | null;
	/** `catalog_load`, `install` or `upgrade`. */
	kind: string;
	plugin: string;
	from_version?: string | null;
	to_version?: string | null;
	/** The organization's database name; empty for a catalog load (it applies to all of them). */
	organization?: string | null;
	actor_kind?: string | null;
	actor_name?: string | null;
	/** Where it was done from: `desk:reload`, `desk:install`, `cli:upgrade`, `watch`, … */
	source?: string | null;
	ok?: boolean | null;
	error?: string | null;
}

/** The plugin activity log of an organization, newest first: its installs and upgrades and every catalog load. */
export async function fetchPluginActivity(
	db: string,
	options: { limit?: number; offset?: number } = {}
): Promise<PluginActivityEvent[]> {
	const params = new URLSearchParams();
	if (options.limit) params.set('limit', String(options.limit));
	if (options.offset) params.set('offset', String(options.offset));
	const query = params.toString();
	const res = await apiFetch(
		`/api/ui/organizations/${encodeURIComponent(db)}/plugin-activity${query ? `?${query}` : ''}`
	);
	if (!res.ok) {
		throw new Error(await readError(res, `The activity could not be loaded (${res.status}).`));
	}
	const body = (await res.json()) as { events?: PluginActivityEvent[] };
	return body.events ?? [];
}
