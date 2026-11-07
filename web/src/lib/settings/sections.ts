import type { CatalogItem } from './api';

/** A labelled block of settings inside a large group (e.g. Communication). */
export type SettingsSection = {
	id: string;
	title: string;
	items: CatalogItem[];
};

type SectionRule = {
	id: string;
	title: string;
	match: (key: string) => boolean;
};

/** Order and titles for Communication (and similar) settings pages. */
const COMMUNICATION_SECTIONS: SectionRule[] = [
	{
		id: 'email',
		title: 'Email',
		match: (key) => key.startsWith('communications.email.')
	},
	{
		id: 'smtp',
		title: 'SMTP',
		match: (key) => key.startsWith('bridge.smtp.')
	},
	{
		id: 'resend',
		title: 'Resend',
		match: (key) => key.startsWith('bridge.resend.')
	},
	{
		id: 'sms',
		title: 'SMS',
		match: (key) => key.startsWith('communications.sms.')
	},
	{
		id: 'twilio',
		title: 'Twilio',
		match: (key) => key.startsWith('bridge.twilio.')
	},
	{
		id: 'termii',
		title: 'Termii',
		match: (key) => key.startsWith('bridge.termii.')
	},
	{
		id: 'africas-talking',
		title: "Africa's Talking",
		match: (key) => key.startsWith('bridge.africas_talking.')
	}
];

/**
 * Split items into labelled sections when the group is Communication; otherwise
 * one untitled section so the page layout stays the same.
 */
export function settingsSections(slug: string, items: CatalogItem[]): SettingsSection[] {
	if (slug !== 'communication') {
		return [{ id: 'all', title: '', items }];
	}

	const remaining = [...items];
	const sections: SettingsSection[] = [];
	for (const rule of COMMUNICATION_SECTIONS) {
		const matched = remaining.filter((item) => rule.match(item.key));
		if (matched.length === 0) continue;
		sections.push({ id: rule.id, title: rule.title, items: matched });
		for (const item of matched) {
			const index = remaining.indexOf(item);
			if (index >= 0) remaining.splice(index, 1);
		}
	}
	if (remaining.length > 0) {
		sections.push({ id: 'other', title: 'Other', items: remaining });
	}
	return sections;
}
