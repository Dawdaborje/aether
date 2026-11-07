import { describe, expect, it } from 'vitest';
import { confirmAction, confirmState } from './confirm.svelte';

describe('confirmAction', () => {
	it('resolves true when confirmed and false when cancelled', async () => {
		const yes = confirmAction({ title: 'Delete?' });
		expect(confirmState.pending?.title).toBe('Delete?');
		confirmState.answer(true);
		expect(await yes).toBe(true);
		expect(confirmState.pending).toBeNull();

		const no = confirmAction({ title: 'Delete?' });
		confirmState.answer(false);
		expect(await no).toBe(false);
	});

	it('cancels the first question when a second one replaces it', async () => {
		const first = confirmAction({ title: 'One' });
		const second = confirmAction({ title: 'Two' });
		expect(await first).toBe(false);
		expect(confirmState.pending?.title).toBe('Two');
		confirmState.answer(true);
		expect(await second).toBe(true);
	});

	it('ignores an answer when nothing is asked', () => {
		expect(() => confirmState.answer(true)).not.toThrow();
	});
});
