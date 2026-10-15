/** Format a catalog `date_created` for version tabs and detail. */
export function formatCatalogDate(iso: string | null | undefined): string | null {
	if (!iso) return null;
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return iso;
	return new Intl.DateTimeFormat(undefined, {
		dateStyle: 'medium',
		timeStyle: 'short',
		timeZone: 'UTC'
	}).format(date) + ' UTC';
}

/** Short change summary: `2 changed · 1 added`. */
export function formatFileChangeSummary(plugin: {
	files_added?: string[];
	files_changed?: string[];
	files_removed?: string[];
}): string | null {
	const added = plugin.files_added?.length ?? 0;
	const changed = plugin.files_changed?.length ?? 0;
	const removed = plugin.files_removed?.length ?? 0;
	const parts: string[] = [];
	if (changed) parts.push(`${changed} changed`);
	if (added) parts.push(`${added} added`);
	if (removed) parts.push(`${removed} removed`);
	return parts.length > 0 ? parts.join(' · ') : null;
}
