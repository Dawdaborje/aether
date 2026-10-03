/**
 * The words for each kind of error, shared by every error-page set so a set
 * only has to decide how things look.
 */
export interface ErrorCopy {
	title: string;
	message: string;
}

const copy: Record<number, ErrorCopy> = {
	401: { title: 'Sign in required', message: 'Please sign in to see this page.' },
	403: {
		title: 'Access denied',
		message: 'You do not have permission to see this page.'
	},
	404: {
		title: 'Page not found',
		message: 'We could not find what you were looking for. It may have moved or never existed.'
	},
	429: {
		title: 'Too many requests',
		message: 'You are going a little fast. Please wait a moment and try again.'
	}
};

const serverError: ErrorCopy = {
	title: 'Something went wrong',
	message: 'An unexpected error occurred. Please try again, or contact an administrator if it keeps happening.'
};

/** The standard title and message for `status`, with any overrides applied. */
export function errorCopy(status: number, overrides: Partial<ErrorCopy> = {}): ErrorCopy {
	const base = copy[status] ?? serverError;
	return {
		title: overrides.title || base.title,
		message: overrides.message || base.message
	};
}
