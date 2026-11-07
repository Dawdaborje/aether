import { describe, expect, it } from 'vitest';
import { parseInline, parseMarkdown } from './markdown';

describe('markdown', () => {
	it('reads blocks', () => {
		const blocks = parseMarkdown(
			'# Title\n\nSome *text*.\n\n- a\n- b\n\n```js\nlet x;\n```\n\n> quoted\n\n---'
		);
		expect(blocks.map((b) => b.t)).toEqual(['heading', 'p', 'list', 'code', 'quote', 'hr']);
		expect(blocks[3]).toEqual({ t: 'code', lang: 'js', v: 'let x;' });
	});

	it('reads a table', () => {
		const [table] = parseMarkdown('| a | b |\n|---|---|\n| 1 | 2 |');
		expect(table.t).toBe('table');
		if (table.t === 'table') expect(table.rows).toHaveLength(1);
	});

	it('links to pages of the app, but not to other sites written without a scheme', () => {
		const link = (src: string) => parseInline(src).find((i) => i.t === 'link');
		expect(link('[a](/wiki/p/abc)')).toMatchObject({ href: '/wiki/p/abc', app: true });
		expect(link('[a](//evil.example/x)')).toBeUndefined();
		expect(link('[a](/\\evil.example)')).toBeUndefined();
		expect(link('[a](relative/path)')).toBeUndefined();
	});

	it('never turns an unsafe link into a link, and never makes markup', () => {
		const inline = parseInline(
			'[x](javascript:alert(1)) <img src=x onerror=alert(1)> [ok](https://a.example)'
		);
		expect(inline.some((i) => i.t === 'link' && i.href.startsWith('javascript'))).toBe(false);
		expect(inline.filter((i) => i.t === 'link')).toHaveLength(1);
		const text = inline
			.filter((i) => i.t === 'text')
			.map((i) => (i as { v: string }).v)
			.join('');
		expect(text).toContain('<img src=x onerror=alert(1)>');
	});

	it('terminates on odd input', () => {
		expect(() =>
			parseMarkdown('**unclosed [link( `tick\n|\n> >>>\n' + '*'.repeat(500))
		).not.toThrow();
	});
});
