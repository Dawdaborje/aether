/** Apps without a workspace are grouped here. */
export const OTHER_CATEGORY = 'Other';

/** The category name shown for a plugin's workspace (`erp_sales` → `Erp Sales`). */
export function categoryLabel(workspace: string | null | undefined): string {
	const name = (workspace ?? '').trim();
	if (!name) return OTHER_CATEGORY;
	return name
		.split(/[\s_-]+/)
		.filter(Boolean)
		.map((word) => word.charAt(0).toUpperCase() + word.slice(1))
		.join(' ');
}
