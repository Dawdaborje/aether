/**
 * Calendar arithmetic for the date picker. Dates travel as `YYYY-MM-DD` (and
 * `YYYY-MM-DDTHH:MM` with a time), exactly what `<input type="date">` and
 * `datetime-local` used, so stored values do not change. Everything here is plain
 * year / month (1-12) / day numbers: no `Date` object is involved in the arithmetic, so a
 * time zone or a daylight-saving change can never move a day.
 */

export interface Ymd {
	y: number;
	/** 1 to 12. */
	m: number;
	d: number;
}

export interface YmdHm extends Ymd {
	hh: number;
	mm: number;
}

const pad = (n: number, width = 2) => String(n).padStart(width, '0');

export function isLeap(y: number): boolean {
	return (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0;
}

export function daysInMonth(y: number, m: number): number {
	if (m === 2) return isLeap(y) ? 29 : 28;
	return [4, 6, 9, 11].includes(m) ? 30 : 31;
}

/** 0 = Sunday … 6 = Saturday (Zeller-free: from a day count, so no `Date` is needed). */
export function weekday(y: number, m: number, d: number): number {
	return (((toDayNumber({ y, m, d }) + 4) % 7) + 7) % 7;
}

/** Days since 1970-01-01 (negative before it). */
export function toDayNumber({ y, m, d }: Ymd): number {
	// Howard Hinnant's days_from_civil.
	const year = m <= 2 ? y - 1 : y;
	const era = Math.floor(year / 400);
	const yoe = year - era * 400;
	const mp = (m + 9) % 12;
	const doy = Math.floor((153 * mp + 2) / 5) + d - 1;
	const doe = yoe * 365 + Math.floor(yoe / 4) - Math.floor(yoe / 100) + doy;
	return era * 146097 + doe - 719468;
}

export function fromDayNumber(days: number): Ymd {
	const z = days + 719468;
	const era = Math.floor(z / 146097);
	const doe = z - era * 146097;
	const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365);
	const y = yoe + era * 400;
	const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100));
	const mp = Math.floor((5 * doy + 2) / 153);
	const d = doy - Math.floor((153 * mp + 2) / 5) + 1;
	const m = mp < 10 ? mp + 3 : mp - 9;
	return { y: m <= 2 ? y + 1 : y, m, d };
}

export function addDays(date: Ymd, days: number): Ymd {
	return fromDayNumber(toDayNumber(date) + days);
}

/** Move by whole months, keeping the day where it exists and otherwise the month's last day. */
export function addMonths(date: Ymd, months: number): Ymd {
	const index = date.y * 12 + (date.m - 1) + months;
	const y = Math.floor(index / 12);
	const m = (index % 12 + 12) % 12 + 1;
	return { y, m, d: Math.min(date.d, daysInMonth(y, m)) };
}

export function compare(a: Ymd, b: Ymd): number {
	return toDayNumber(a) - toDayNumber(b);
}

export function sameDay(a: Ymd | null, b: Ymd | null): boolean {
	return !!a && !!b && a.y === b.y && a.m === b.m && a.d === b.d;
}

export function isValid({ y, m, d }: Ymd): boolean {
	return Number.isInteger(y) && y >= 1 && y <= 9999 && m >= 1 && m <= 12 && d >= 1 && d <= daysInMonth(y, m);
}

/** `2026-10-15` or `2026-10-15T09:30` (a space for the `T` and trailing seconds are tolerated). */
export function parseValue(value: unknown): YmdHm | null {
	if (typeof value !== 'string') return null;
	const match = /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?)?$/.exec(
		value.trim()
	);
	if (!match) return null;
	const out = {
		y: Number(match[1]),
		m: Number(match[2]),
		d: Number(match[3]),
		hh: match[4] ? Number(match[4]) : 0,
		mm: match[5] ? Number(match[5]) : 0
	};
	if (!isValid(out) || out.hh > 23 || out.mm > 59) return null;
	return out;
}

export function formatDate({ y, m, d }: Ymd): string {
	return `${pad(y, 4)}-${pad(m)}-${pad(d)}`;
}

export function formatDateTime(value: YmdHm): string {
	return `${formatDate(value)}T${pad(value.hh)}:${pad(value.mm)}`;
}

/** The 42 cells (six weeks) of a month's grid, starting on `firstDay` (0 Sunday, 1 Monday …). */
export function monthGrid(y: number, m: number, firstDay: number): (Ymd & { inMonth: boolean })[] {
	const lead = (weekday(y, m, 1) - firstDay + 7) % 7;
	const start = addDays({ y, m, d: 1 }, -lead);
	const cells: (Ymd & { inMonth: boolean })[] = [];
	for (let i = 0; i < 42; i++) {
		const day = addDays(start, i);
		cells.push({ ...day, inMonth: day.m === m && day.y === y });
	}
	return cells;
}

/**
 * Read what a person typed: `2026-10-15`, `15/10/2026`, `15.10.2026`, `15-10-2026`, or
 * `15 Oct 2026` / `Oct 15, 2026`. A slash date is day first unless the first number cannot be a
 * day (`10/25/2026` is read month first). `null` when it is not a date.
 */
export function parseTyped(text: string): Ymd | null {
	const raw = text.trim();
	if (!raw) return null;
	const iso = /^(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})$/.exec(raw);
	if (iso) {
		const out = { y: Number(iso[1]), m: Number(iso[2]), d: Number(iso[3]) };
		return isValid(out) ? out : null;
	}
	const dmy = /^(\d{1,2})[-/.](\d{1,2})[-/.](\d{4})$/.exec(raw);
	if (dmy) {
		const a = Number(dmy[1]);
		const b = Number(dmy[2]);
		const y = Number(dmy[3]);
		const dayFirst = { y, m: b, d: a };
		if (isValid(dayFirst)) return dayFirst;
		const monthFirst = { y, m: a, d: b };
		return isValid(monthFirst) ? monthFirst : null;
	}
	const months = ['jan', 'feb', 'mar', 'apr', 'may', 'jun', 'jul', 'aug', 'sep', 'oct', 'nov', 'dec'];
	const named = /^(\d{1,2})\s+([A-Za-z]{3,})\.?,?\s+(\d{4})$/.exec(raw);
	if (named) {
		const m = months.indexOf(named[2]!.slice(0, 3).toLowerCase()) + 1;
		const out = { y: Number(named[3]), m, d: Number(named[1]) };
		return m > 0 && isValid(out) ? out : null;
	}
	const monthFirst = /^([A-Za-z]{3,})\.?\s+(\d{1,2}),?\s+(\d{4})$/.exec(raw);
	if (monthFirst) {
		const m = months.indexOf(monthFirst[1]!.slice(0, 3).toLowerCase()) + 1;
		const out = { y: Number(monthFirst[3]), m, d: Number(monthFirst[2]) };
		return m > 0 && isValid(out) ? out : null;
	}
	return null;
}

/** The first day of the week for a locale (`Intl.Locale#weekInfo`), Monday when unknown. */
export function localeFirstDay(locale?: string): number {
	try {
		const info = new Intl.Locale(locale ?? navigator.language) as Intl.Locale & {
			weekInfo?: { firstDay: number };
			getWeekInfo?: () => { firstDay: number };
		};
		const first = (info.getWeekInfo?.() ?? info.weekInfo)?.firstDay;
		if (typeof first === 'number') return first % 7; // the API counts Sunday as 7
	} catch {
		// fall through
	}
	return 1;
}
