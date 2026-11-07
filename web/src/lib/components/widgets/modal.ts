import { getContext, setContext } from 'svelte';

/** What a widget inside a `<modal>` can ask of it. */
export interface ModalState {
	close: () => void;
	/** The row a list opened this modal on (`edit="…"`); `null` when its own button opened it. */
	readonly editing: Record<string, unknown> | null;
}

const key = Symbol('aether.modal');

export function setModalState(state: ModalState): void {
	setContext(key, state);
}

/** The modal around this widget, if it is in one. */
export function getModalState(): ModalState | undefined {
	return getContext<ModalState | undefined>(key);
}
