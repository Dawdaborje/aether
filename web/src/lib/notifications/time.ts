const units: [Intl.RelativeTimeFormatUnit, number][] = [
	['day', 86_400],
	['hour', 3_600],
	['minute', 60]
];

const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });

/** "5 minutes ago", "yesterday"; "just now" under a minute. */
export function timeAgo(iso: string, now = Date.now()): string {
	const seconds = Math.round((Date.parse(iso) - now) / 1000);
	if (Number.isNaN(seconds)) return '';
	const distance = Math.abs(seconds);
	for (const [unit, size] of units) {
		if (distance >= size) return formatter.format(Math.round(seconds / size), unit);
	}
	return 'just now';
}
