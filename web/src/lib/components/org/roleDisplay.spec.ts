import { describe, expect, it } from 'vitest';
import { humanizeRoleName, kindLabel, resolveRoleDisplay, roleKind } from './roleDisplay';

describe('roleDisplay', () => {
	it('classifies role kinds', () => {
		expect(roleKind('org_admin')).toBe('system');
		expect(roleKind('user_manager')).toBe('system');
		expect(roleKind('hr.hr_manager', true)).toBe('plugin');
		expect(roleKind('custom.shift_lead')).toBe('custom');
	});

	it('resolves labels from the catalog', () => {
		const display = resolveRoleDisplay('hr.hr_manager', [
			{
				name: 'hr.hr_manager',
				label: 'HR manager',
				description: 'Hires people',
				is_system: true,
				holders: []
			}
		]);
		expect(display.label).toBe('HR manager');
		expect(display.kind).toBe('plugin');
		expect(kindLabel(display.kind)).toBe('App');
	});

	it('humanizes unknown names', () => {
		expect(humanizeRoleName('custom.shift_lead')).toBe('Shift Lead');
		expect(resolveRoleDisplay('custom.floor_lead', []).label).toBe('Floor Lead');
	});
});
