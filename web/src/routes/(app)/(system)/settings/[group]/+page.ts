import { redirect } from '@sveltejs/kit';
import { base } from '$app/paths';
import type { PageLoad } from './$types';

export const prerender = false;

/** Old path; the desk lives under `/desk`. */
export const load: PageLoad = ({ params }) => {
	redirect(308, `${base}/desk/settings/${params.group}`);
};
