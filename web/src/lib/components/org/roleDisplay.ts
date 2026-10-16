import type { OrgRole } from '$lib/org/api';

export type RoleKind = 'system' | 'plugin' | 'custom';

export interface RoleDisplay {
	name: string;
	label: string;
	kind: RoleKind;
	description?: string | null;
	is_system: boolean;
}

export function roleKind(name: string, isSystem?: boolean): RoleKind {
	if (name === 'org_admin' || name === 'user_manager') return 'system';
	if (name.startsWith('custom.')) return 'custom';
	if (isSystem === false) return 'custom';
	if (name.includes('.')) return 'plugin';
	return isSystem === false ? 'custom' : 'system';
}

export function kindLabel(kind: RoleKind): string {
	switch (kind) {
		case 'system':
			return 'System';
		case 'plugin':
			return 'App';
		case 'custom':
			return 'Custom';
	}
}

/** Humanize `hr.hr_manager` → `Hr manager` when no catalog label is available. */
export function humanizeRoleName(name: string): string {
	const segment = name.includes('.') ? (name.split('.').pop() ?? name) : name;
	return segment
		.replace(/_/g, ' ')
		.replace(/\b\w/g, (c) => c.toUpperCase());
}

export function resolveRoleDisplay(
	name: string,
	catalog: OrgRole[] | undefined | null
): RoleDisplay {
	const found = catalog?.find((role) => role.name === name);
	if (found) {
		const kind = roleKind(found.name, found.is_system);
		return {
			name: found.name,
			label: found.label || humanizeRoleName(found.name),
			kind,
			description: found.description,
			is_system: found.is_system
		};
	}
	const kind = roleKind(name);
	return {
		name,
		label: humanizeRoleName(name),
		kind,
		description: null,
		is_system: kind !== 'custom'
	};
}

/** Lower case, with the separators role keys use (`.`, `_`, `-`) read as spaces and accents dropped. */
function normalizeForSearch(text: string): string {
	return text
		.normalize('NFD')
		.replace(/[̀-ͯ]/g, '')
		.toLowerCase()
		.replace(/[._-]+/g, ' ');
}

/**
 * Whether every word of `query` appears somewhere in `parts` (a role's key, label, description…).
 * Keys are written `hr_leave.leave_approver` but people type "hr leave approver", so separators
 * count as spaces and the words may come from different fields in any order.
 */
export function matchesSearch(parts: (string | null | undefined)[], query: string): boolean {
	const words = normalizeForSearch(query).split(/\s+/).filter(Boolean);
	if (words.length === 0) return true;
	const haystack = normalizeForSearch(parts.filter(Boolean).join(' '));
	return words.every((word) => haystack.includes(word));
}
