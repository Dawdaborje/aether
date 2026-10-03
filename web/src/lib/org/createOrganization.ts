import { apiFetch } from '$lib/api/client';

/** Who the new organization starts with. */
export type NewMember =
	| { mode: 'new'; username: string; email: string; password: string }
	| { mode: 'existing'; login: string };

export interface CreatedOrganization {
	organization: { name: string; db_name: string };
	/** Where the organization's files live on the server. */
	files: string;
}

/**
 * The database name the server will make from an organization's name: lowercase
 * letters and digits, everything else collapsed to single underscores.
 */
export function identifierFor(name: string): string {
	return name
		.trim()
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '_')
		.replace(/^_+|_+$/g, '');
}

/** Create an organization (developers only). Throws an Error whose message is for the user. */
export async function createOrganization(input: {
	name: string;
	identifier: string;
	member: NewMember;
}): Promise<CreatedOrganization> {
	const res = await apiFetch('/api/ui/organizations', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({
			name: input.name,
			identifier: input.identifier || undefined,
			member: input.member
		})
	});
	if (!res.ok) {
		const body = await res.json().catch(() => ({}));
		throw new Error(
			typeof body.error === 'string' ? body.error : `The organization could not be created (${res.status}).`
		);
	}
	return (await res.json()) as CreatedOrganization;
}
