/**
 * Ask before something is changed or removed, without a dialog in every component:
 *
 *   if (!(await confirmAction({ title: 'Delete this message?', destructive: true }))) return;
 *
 * One `ConfirmHost` (mounted in the root layout) shows the question. It resolves `true` when
 * the person confirms and `false` when they cancel, press Escape or click away.
 */

export type ConfirmRequest = {
	title: string;
	description?: string;
	confirmLabel?: string;
	/** Style the button as dangerous, for something that cannot be undone. */
	destructive?: boolean;
};

type Pending = ConfirmRequest & { resolve: (answer: boolean) => void };

class ConfirmState {
	pending = $state<Pending | null>(null);

	ask(request: ConfirmRequest): Promise<boolean> {
		// A second question while one is open replaces it; the first counts as cancelled.
		this.pending?.resolve(false);
		return new Promise<boolean>((resolve) => {
			this.pending = { ...request, resolve };
		});
	}

	answer(value: boolean) {
		const current = this.pending;
		this.pending = null;
		current?.resolve(value);
	}
}

export const confirmState = new ConfirmState();

export function confirmAction(request: ConfirmRequest): Promise<boolean> {
	return confirmState.ask(request);
}
