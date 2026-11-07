import { describe, expect, it } from 'vitest';
import type { CatalogItem } from './api';
import { settingsSections } from './sections';

function item(key: string, label = key): CatalogItem {
	return {
		key,
		label,
		value: '',
		source: 'global',
		value_type: 'string'
	};
}

describe('settingsSections', () => {
	it('keeps a flat group as one untitled section', () => {
		const items = [item('app_name', 'Application Name')];
		expect(settingsSections('general', items)).toEqual([
			{ id: 'all', title: '', items }
		]);
	});

	it('splits Communication into email, providers, then SMS', () => {
		const items = [
			item('bridge.twilio.from'),
			item('communications.email.provider'),
			item('bridge.smtp.host'),
			item('communications.sms.provider')
		];
		const sections = settingsSections('communication', items);
		expect(sections.map((s) => s.id)).toEqual(['email', 'smtp', 'sms', 'twilio']);
		expect(sections[0]!.items.map((i) => i.key)).toEqual(['communications.email.provider']);
		expect(sections[1]!.items.map((i) => i.key)).toEqual(['bridge.smtp.host']);
	});
});
