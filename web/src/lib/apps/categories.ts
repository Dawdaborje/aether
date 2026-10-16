/** Apps without a workspace are grouped here. */
export const OTHER_CATEGORY = 'Other';

/** Short names shown in full caps (`crm` → `CRM`, `erp_sales` → `ERP Sales`). */
const ACRONYMS = new Set([
	'api',
	'crm',
	'erp',
	'hr',
	'hrms',
	'pos',
	'ui',
	'uom'
]);

/** The category name shown for a plugin's workspace (`erp_sales` → `ERP Sales`). */
export function categoryLabel(workspace: string | null | undefined): string {
	const name = (workspace ?? '').trim();
	if (!name) return OTHER_CATEGORY;
	return name
		.split(/[\s_-]+/)
		.filter(Boolean)
		.map((word) => {
			const lower = word.toLowerCase();
			if (ACRONYMS.has(lower)) return lower.toUpperCase();
			return lower.charAt(0).toUpperCase() + lower.slice(1);
		})
		.join(' ');
}

/** Lucide icons for well-known workspaces, shown when a group has no logo of its own. */
const CATEGORY_ICONS: Record<string, string> = {
	hrms: 'users',
	hr: 'users',
	erp: 'boxes',
	crm: 'handshake',
	accounting: 'banknote',
	base: 'package'
};

/** The icon for a group with no logo: its workspace's own, else a plain folder. */
export function categoryIcon(workspace: string | null | undefined): string {
	const key = (workspace ?? '').trim().toLowerCase();
	return CATEGORY_ICONS[key] ?? 'folder';
}
