/** How the people of a meeting are laid out; pure, so it can be tested without a browser. */

import type { Participant } from './client';

export interface Tile {
	person: Participant;
	you: boolean;
}

/** Columns for `count` tiles: as square as the room allows, never more than `max`. */
export function columnsFor(count: number, max = 4): number {
	if (count <= 1) return 1;
	return Math.min(max, Math.ceil(Math.sqrt(count)));
}

/**
 * The order tiles are drawn in: the pinned person first, then people who are speaking, then you, then the rest in the
 * order they joined. A stable order keeps tiles from jumping around.
 */
export function orderTiles(roster: Participant[], you: string, speaking: ReadonlySet<string>, pinned: string | null): Tile[] {
	const rank = (p: Participant): number => {
		if (p.user === pinned) return 0;
		if (speaking.has(p.sid)) return 1;
		if (p.user === you) return 2;
		return 3;
	};
	return [...roster]
		.sort((a, b) => rank(a) - rank(b) || a.since - b.since || a.user.localeCompare(b.user))
		.map((person) => ({ person, you: person.user === you }));
}

/** One or two letters for a tile with no picture. */
export function initials(name: string): string {
	const words = name.trim().split(/\s+/).filter(Boolean);
	if (words.length === 0) return '?';
	if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
	return (words[0][0] + words[words.length - 1][0]).toUpperCase();
}
