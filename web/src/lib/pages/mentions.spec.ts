import { describe, expect, it } from 'vitest';
import { applyMention, mentionQuery, splitMentions } from './mentions';

describe('mentions', () => {
	it('marks the names that are mentioned, and @channel', () => {
		const parts = splitMentions('hi @Ann Lee and @bob, @channel!', ['Ann Lee', 'Bob']);
		expect(parts.filter((p) => p.mention).map((p) => p.text)).toEqual([
			'@Ann Lee',
			'@bob',
			'@channel'
		]);
		expect(parts.map((p) => p.text).join('')).toBe('hi @Ann Lee and @bob, @channel!');
	});

	it('does not mark half a name, an email address or someone not named', () => {
		expect(splitMentions('@bobby', ['Bob']).some((p) => p.mention)).toBe(false);
		expect(splitMentions('mail a@bob.example', ['Bob']).some((p) => p.mention)).toBe(false);
		expect(splitMentions('@zed', ['Bob']).some((p) => p.mention)).toBe(false);
		expect(splitMentions('', [])).toEqual([{ text: '', mention: false }]);
	});

	it('takes a name with regex characters literally', () => {
		expect(splitMentions('@A.B (x)', ['A.B (x)']).filter((p) => p.mention)).toHaveLength(1);
		expect(splitMentions('@AxB', ['A.B']).some((p) => p.mention)).toBe(false);
	});

	it('finds the @query being typed', () => {
		expect(mentionQuery('hello @an', 9)).toEqual({ start: 6, query: 'an' });
		expect(mentionQuery('@', 1)).toEqual({ start: 0, query: '' });
		expect(mentionQuery('mail me at a@b', 14)).toBeNull();
		expect(mentionQuery('no mention', 10)).toBeNull();
		expect(mentionQuery('@ann\nnext', 9)).toBeNull();
		expect(mentionQuery('@ann lee', 8)).toEqual({ start: 0, query: 'ann lee' });
	});

	it('completes the name and moves the caret after it', () => {
		const done = applyMention('hello @an rest', 9, 6, 'Ann Lee');
		expect(done.text).toBe('hello @Ann Lee  rest');
		expect(done.caret).toBe(15);
	});
});
