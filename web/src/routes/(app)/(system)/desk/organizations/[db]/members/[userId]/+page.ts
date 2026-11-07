import { redirect } from '@sveltejs/kit';
import { resolve } from '$app/paths';
import type { PageLoad } from './$types';

/** Member detail lives on the standalone Users page, not inside org tabs. */
export const load: PageLoad = ({ params }) => {
	const path = resolve(`/desk/users/${encodeURIComponent(params.userId)}`);
	redirect(308, `${path}?org=${encodeURIComponent(params.db)}`);
};
