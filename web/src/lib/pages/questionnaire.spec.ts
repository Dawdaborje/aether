import { describe, expect, it } from 'vitest';
import { clock, firstMissing, isAnswered, payload, readQuestionnaire, toggle, type Question } from './questionnaire';

const q = (id: string, kind = 'text', required = false): Question => ({ id, label: id, kind, required, help: '', options: [], points: null });

describe('questionnaire', () => {
	it('reads the questions, the lock and the thanks', () => {
		const r = readQuestionnaire({ questions: [{ id: 'a', label: 'A', kind: 'choice', options: ['x', 'y'], required: true }], locked: 'Closed.', thanks: '' });
		expect(r.questions[0]).toMatchObject({ id: 'a', kind: 'choice', options: ['x', 'y'], required: true });
		expect(r.locked).toBe('Closed.');
		expect(r.thanks).toBe('Thank you.');
	});

	it('takes a bare list of questions too, and survives nonsense', () => {
		expect(readQuestionnaire([{ id: 'a' }]).questions).toHaveLength(1);
		expect(readQuestionnaire(null).questions).toEqual([]);
	});

	it('counts blank text and empty lists as not answered, and 0 as answered', () => {
		expect(isAnswered('  ')).toBe(false);
		expect(isAnswered([])).toBe(false);
		expect(isAnswered(undefined)).toBe(false);
		expect(isAnswered(0)).toBe(true);
		expect(isAnswered('x')).toBe(true);
	});

	it('finds the first required question without an answer', () => {
		const qs = [q('a'), q('b', 'text', true), q('c', 'text', true)];
		expect(firstMissing(qs, { c: 'x' })?.id).toBe('b');
		expect(firstMissing(qs, { b: 'x', c: 'y' })).toBeNull();
	});

	it('sends only what was answered, trimmed', () => {
		expect(payload([q('a'), q('b'), q('c')], { a: ' hi ', b: '', c: 3 })).toEqual({ a: 'hi', c: 3 });
	});

	it('toggles one choice at a time', () => {
		expect(toggle(undefined, 'a', true)).toEqual(['a']);
		expect(toggle(['a'], 'b', true)).toEqual(['a', 'b']);
		expect(toggle(['a', 'b'], 'a', false)).toEqual(['b']);
		expect(toggle(['a'], 'a', true)).toEqual(['a']);
	});

	it('reads a timed attempt: the clock, the marks and the answers already saved', () => {
		const r = readQuestionnaire({ questions: [{ id: 'a', points: 2 }], seconds_left: 125.9, saved: { a: 'x', b: ['y'], c: { no: 1 } } });
		expect(r.secondsLeft).toBe(125);
		expect(r.questions[0].points).toBe(2);
		expect(r.saved).toEqual({ a: 'x', b: ['y'] });
		expect(readQuestionnaire({ questions: [] }).secondsLeft).toBeNull();
	});

	it('shows the clock as minutes and seconds, with hours from an hour up', () => {
		expect(clock(245)).toBe('4:05');
		expect(clock(3723)).toBe('1:02:03');
		expect(clock(-4)).toBe('0:00');
	});
});
