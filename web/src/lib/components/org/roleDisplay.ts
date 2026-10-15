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
