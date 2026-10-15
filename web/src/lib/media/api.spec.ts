import { describe, expect, it } from 'vitest';
import { filesOf, formatSize, isMediaFile, legacyImageSrc } from './api';

const file = { key: 'media/2026/10/ab.png', name: 'a.png', mime: 'image/png', size: 10 };

describe('media values', () => {
	it('recognises what an upload returns', () => {
		expect(isMediaFile(file)).toBe(true);
		expect(isMediaFile({ ...file, key: 'plugins/x/a.png' })).toBe(false);
		expect(isMediaFile('media/2026/a.png')).toBe(false);
		expect(isMediaFile(null)).toBe(false);
	});

	it('reads one file, a list, or nothing', () => {
		expect(filesOf(file)).toEqual([file]);
		expect(filesOf([file, 'old text', { key: 5 }])).toEqual([file]);
		expect(filesOf(null)).toEqual([]);
		expect(filesOf('data:image/png;base64,AAAA')).toEqual([]);
	});

	it('shows an old picture value only when it is safe to load', () => {
		expect(legacyImageSrc('data:image/png;base64,AAAA')).toBe('data:image/png;base64,AAAA');
		expect(legacyImageSrc('https://cdn.example/a.png')).toBe('https://cdn.example/a.png');
		expect(legacyImageSrc('javascript:alert(1)')).toBe('');
		expect(legacyImageSrc('data:text/html;base64,AAAA')).toBe('');
		expect(legacyImageSrc(file)).toBe('');
	});

	it('formats sizes', () => {
		expect(formatSize(512)).toBe('512 B');
		expect(formatSize(2048)).toBe('2 KB');
		expect(formatSize(5 * 1024 * 1024)).toBe('5.0 MB');
	});
});
