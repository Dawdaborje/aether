/** The questions a `<questionnaire>` shows, and what it sends back. */

export interface Question {
	id: string;
	label: string;
	kind: string;
	required: boolean;
	help: string;
	options: string[];
	/** What the question is worth, shown beside it (a quiz). */
	points: number | null;
}

export interface Questionnaire {
	questions: Question[];
	/** Why there is nothing to answer (closed, already answered, a draft). */
	locked: string | null;
	intro: string;
	anonymous: boolean;
	thanks: string;
	/** Seconds the server says are left for a timed questionnaire (a quiz attempt); null when there is no clock. */
	secondsLeft: number | null;
	/** Answers already given and saved, to carry on from. */
	saved: Record<string, Answer>;
}

export type Answer = string | string[] | number | undefined;

export function readQuestionnaire(data: unknown): Questionnaire {
	const d = (data && typeof data === 'object' ? data : {}) as Record<string, unknown>;
	const list = Array.isArray(data) ? data : Array.isArray(d.questions) ? d.questions : [];
	return {
		questions: (list as Record<string, unknown>[])
			.filter((q) => q && typeof q === 'object')
			.map((q) => ({
				id: String(q.id ?? ''),
				label: String(q.label ?? ''),
				kind: String(q.kind ?? 'text'),
				required: q.required === true,
				help: typeof q.help === 'string' ? q.help : '',
				options: Array.isArray(q.options) ? q.options.map(String) : [],
				points: typeof q.points === 'number' ? q.points : null
			})),
		locked: typeof d.locked === 'string' && d.locked !== '' ? d.locked : null,
		intro: typeof d.intro === 'string' ? d.intro : '',
		anonymous: d.anonymous === true,
		thanks: typeof d.thanks === 'string' && d.thanks !== '' ? d.thanks : 'Thank you.',
		secondsLeft: typeof d.seconds_left === 'number' ? Math.max(0, Math.floor(d.seconds_left)) : null,
		saved: readSaved(d.saved)
	};
}

function readSaved(value: unknown): Record<string, Answer> {
	const out: Record<string, Answer> = {};
	if (!value || typeof value !== 'object' || Array.isArray(value)) return out;
	for (const [id, v] of Object.entries(value as Record<string, unknown>)) {
		if (typeof v === 'string' || typeof v === 'number') out[id] = v;
		else if (Array.isArray(v)) out[id] = v.map(String);
	}
	return out;
}

/** Clock text for the seconds left: `4:05`, or `1:02:03` from an hour up. */
export function clock(seconds: number): string {
	const s = Math.max(0, Math.floor(seconds));
	const h = Math.floor(s / 3600);
	const m = Math.floor((s % 3600) / 60);
	const r = s % 60;
	const two = (n: number) => String(n).padStart(2, '0');
	return h > 0 ? `${h}:${two(m)}:${two(r)}` : `${m}:${two(r)}`;
}

/** Whether an answer counts as given. */
export function isAnswered(value: Answer): boolean {
	if (value === undefined) return false;
	if (Array.isArray(value)) return value.length > 0;
	if (typeof value === 'number') return true;
	return value.trim() !== '';
}

/** The first required question left unanswered, if any. */
export function firstMissing(questions: Question[], answers: Record<string, Answer>): Question | null {
	return questions.find((q) => q.required && !isAnswered(answers[q.id])) ?? null;
}

/** What is sent: only the questions that have an answer. */
export function payload(questions: Question[], answers: Record<string, Answer>): Record<string, Answer> {
	const out: Record<string, Answer> = {};
	for (const q of questions) {
		const v = answers[q.id];
		if (isAnswered(v)) out[q.id] = typeof v === 'string' ? v.trim() : v;
	}
	return out;
}

/** Add or take away one choice of a multiple-choice answer. */
export function toggle(current: Answer, option: string, on: boolean): string[] {
	const list = Array.isArray(current) ? current : [];
	return on ? (list.includes(option) ? list : [...list, option]) : list.filter((o) => o !== option);
}
