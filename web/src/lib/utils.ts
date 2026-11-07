import { clsx, type ClassValue } from 'clsx';
import { extendTailwindMerge } from 'tailwind-merge';
import { SHAPE_ROLES, TYPE_ROLES } from '$lib/theme/types';

/** Tailwind merge, told about the theme's `type-<role>` and `rounded-<role>` utilities. */
const twMerge = extendTailwindMerge<"type-role">({
	extend: {
		classGroups: {
			'type-role': [{ type: [...TYPE_ROLES] }],
			rounded: [{ rounded: [...SHAPE_ROLES] }],
			duration: [{ duration: ['short', 'medium'] }]
		}
	}
});

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type WithoutChild<T> = T extends { child?: any } ? Omit<T, 'child'> : T;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type WithoutChildren<T> = T extends { children?: any } ? Omit<T, 'children'> : T;
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>;
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null };
