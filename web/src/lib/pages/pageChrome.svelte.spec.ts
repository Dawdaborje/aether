import { page } from 'vitest/browser';
import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import Harness from './pageChrome.harness.svelte';

describe('PageChrome refresh registration', () => {
	it('publishes group-by options and the refresh model without an effect loop', async () => {
		render(Harness);

		await expect.element(page.getByTestId('models')).toHaveTextContent('employee');
		await expect.element(page.getByTestId('groups')).toHaveTextContent('department');
	});
});
