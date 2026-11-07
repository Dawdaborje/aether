import type { Component } from 'svelte';
import Custom404 from './custom/Custom404.svelte';
import CustomErrorCard from './custom/CustomErrorCard.svelte';
import CustomForbidden from './custom/CustomForbidden.svelte';
import CustomServerError from './custom/CustomServerError.svelte';
import CustomTooManyRequests from './custom/CustomTooManyRequests.svelte';
import Default404 from './default/Default404.svelte';
import DefaultErrorCard from './default/DefaultErrorCard.svelte';
import DefaultForbidden from './default/DefaultForbidden.svelte';
import DefaultServerError from './default/DefaultServerError.svelte';
import DefaultTooManyRequests from './default/DefaultTooManyRequests.svelte';
import type { ErrorCardProps, ErrorPageProps } from './types';

type ErrorPage = Component<Partial<ErrorPageProps>>;

/** One look for every kind of error the app can show. */
export interface ErrorSet {
	/** 404 */
	notFound: ErrorPage;
	/** 401 and 403 */
	forbidden: ErrorPage;
	/** 429 */
	tooManyRequests: ErrorPage;
	/** 5xx, and anything else unexpected */
	serverError: ErrorPage;
	/** A small inline card for a failure inside a page (a widget that cannot render). */
	card: Component<ErrorCardProps>;
}

/**
 * Error-page sets a theme can choose by name (`"error_pages": "…"` in its
 * theme.json). To add one, create its components and list it here.
 */
export const errorSets: Record<string, ErrorSet> = {
	default: {
		notFound: Default404 as ErrorPage,
		forbidden: DefaultForbidden as ErrorPage,
		tooManyRequests: DefaultTooManyRequests as ErrorPage,
		serverError: DefaultServerError as ErrorPage,
		card: DefaultErrorCard
	},
	custom: {
		notFound: Custom404 as ErrorPage,
		forbidden: CustomForbidden as ErrorPage,
		tooManyRequests: CustomTooManyRequests as ErrorPage,
		serverError: CustomServerError as ErrorPage,
		card: CustomErrorCard
	}
};

/** The set named `name`; an unknown name falls back to `default`. */
export function errorSetFor(name: string): ErrorSet {
	const set = errorSets[name];
	if (!set) {
		console.warn(
			`Unknown error-page set "${name}"; using "default". Known: ${Object.keys(errorSets).join(', ')}`
		);
		return errorSets.default;
	}
	return set;
}

/** Which page of a set shows `status`. */
export function pageFor(set: ErrorSet, status: number): ErrorPage {
	if (status === 404) return set.notFound;
	if (status === 401 || status === 403) return set.forbidden;
	if (status === 429) return set.tooManyRequests;
	return set.serverError;
}
