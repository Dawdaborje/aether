import { describe, expect, it } from 'vitest';
import { topicName } from './liveTopic.svelte';

const page = (vars: Record<string, string>, plugin = 'chat') => ({ plugin, vars });

describe('topicName', () => {
	it('fills the page values and puts the plugin in front', () => {
		expect(topicName(page({ channel: '7' }), 'channel.{channel}')).toBe('chat:channel.7');
		expect(topicName(page({ 'query.thread': 't1' }), 'thread.{query.thread}')).toBe(
			'chat:thread.t1'
		);
	});
	it('waits while a value is missing', () => {
		expect(topicName(page({}), 'channel.{channel}')).toBeNull();
		expect(topicName(page({ channel: '' }), 'channel.{channel}')).toBeNull();
	});
	it('keeps another plugin in the name and ignores an empty attribute', () => {
		expect(topicName(page({}), 'other:inbox')).toBe('other:inbox');
		expect(topicName(page({}), '')).toBeNull();
		expect(topicName(page({}), undefined)).toBeNull();
		expect(topicName(undefined, 'x')).toBeNull();
	});
});
