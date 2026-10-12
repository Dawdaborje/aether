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
