/** Breadcrumb trails for the desk pages that are nested several levels deep. */

export type Crumb = { label: string; href?: string };

const ORG_SECTIONS: Record<string, string> = {
	overview: 'Overview',
	members: 'Members',
	roles: 'Roles',
	schedules: 'Schedules',
	plugins: 'Plugins',
	audit: 'Audit log',
	'plugin-activity': 'Plugin activity'
};

function decoded(segment: string): string {
	try {
		return decodeURIComponent(segment);
	} catch {
		return segment;
	}
}

/**
 * The trail for a page under `/desk/organizations/<db>/…`:
 * Organizations › Acme › Roles › editor. The overview page is the organization itself, so
 * its trail stops at the organization name. A single-organization installation has no
 * "Organizations" list to go back to.
 */
export function orgCrumbs(opts: {
	single: boolean;
	name: string;
	db: string;
	pathname: string;
}): Crumb[] {
	const base = `/desk/organizations/${opts.db}`;
	const after = opts.pathname.split(`/organizations/${opts.db}/`)[1] ?? '';
	const [section, ...rest] = after.split('/').filter(Boolean);
	const sectionLabel = section && section !== 'overview' ? ORG_SECTIONS[section] : undefined;

	const trail: Crumb[] = opts.single
		? []
		: [{ label: 'Organizations', href: '/desk/organizations' }];
	if (!sectionLabel) return [...trail, { label: opts.name }];

	trail.push({ label: opts.name, href: `${base}/overview` });
	const detail = section === 'roles' && rest[0] ? decoded(rest[0]) : undefined;
	if (!detail) return [...trail, { label: sectionLabel }];
	return [...trail, { label: sectionLabel, href: `${base}/${section}` }, { label: detail }];
}

/** Settings › General (or Appearance), from `/desk/settings/<group>`. */
export function settingsCrumbs(
	pathname: string,
	groups: { slug: string; label: string }[]
): Crumb[] {
	const slug = pathname.split('/desk/settings/')[1]?.split('/')[0];
	if (!slug) return [{ label: 'Settings' }];
	const label = slug === 'appearance' ? 'Appearance' : groups.find((g) => g.slug === slug)?.label;
	return [{ label: 'Settings', href: '/desk/settings' }, { label: label ?? decoded(slug) }];
}
