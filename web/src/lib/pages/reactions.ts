/** Reactions on a message: `{ "👍": ["users:ann", ...] }`, shown as chips with a count. */
export const QUICK_REACTIONS = ['👍', '❤️', '😂', '🎉', '😮', '🙏', '👀', '✅'];

export interface Chip {
	emoji: string;
	count: number;
	mine: boolean;
	/** Who reacted, for the tooltip. */
	users: string[];
}

const keyOf = (id: unknown) => {
	const t = String(id ?? '');
	return t.includes(':') ? t.slice(t.indexOf(':') + 1) : t;
};

/** The chips for a message's `reactions` value, in the order the emoji were first used; `me` is the key of the viewer. */
export function chipsOf(reactions: unknown, me: string): Chip[] {
	if (!reactions || typeof reactions !== 'object' || Array.isArray(reactions)) return [];
	const out: Chip[] = [];
	for (const [emoji, users] of Object.entries(reactions as Record<string, unknown>)) {
		if (!Array.isArray(users) || users.length === 0) continue;
		const list = users.map(String);
		out.push({ emoji, count: list.length, mine: list.some((u) => keyOf(u) === me), users: list });
	}
	return out;
}
