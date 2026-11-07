import { redirect } from '@sveltejs/kit';
import { resolve } from '$app/paths';
import type { PageLoad } from './$types';

export const prerender = false;

/** Old path; the desk lives under `/desk`. */
export const load: PageLoad = () => {
	redirect(308, resolve('/desk/settings/appearance'));
};
