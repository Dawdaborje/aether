import { describe, expect, it } from 'vitest';
import { decodeWords, parseEmail } from './eml';

const plain = `From: A <a@x.test>\r\nTo: b@x.test\r\nSubject: =?utf-8?B?SGVsbG8gd29ybGQ=?=\r\nDate: Mon, 1 Jan 2026 10:00:00 +0000\r\n\r\nHi there\r\n`;

const multi = `From: a@x.test\nSubject: Report\nContent-Type: multipart/mixed; boundary="B1"\n\n--B1\nContent-Type: multipart/alternative; boundary="B2"\n\n--B2\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: quoted-printable\n\nCaf=C3=A9 menu\n--B2\nContent-Type: text/html\n\n<p>Caf&eacute;</p>\n--B2--\n--B1\nContent-Type: application/pdf; name="r.pdf"\nContent-Disposition: attachment; filename="r.pdf"\nContent-Transfer-Encoding: base64\n\nAAAA\n--B1--\n`;

describe('eml', () => {
	it('reads a plain message and decodes the subject', () => {
		const mail = parseEmail(plain);
		expect(mail.headers.subject).toBe('Hello world');
		expect(mail.text).toBe('Hi there');
	});
	it('reads multipart bodies and lists attachments without opening them', () => {
		const mail = parseEmail(multi);
		expect(mail.text).toBe('Café menu');
		expect(mail.html).toContain('<p>');
		expect(mail.attachments).toEqual([{ name: 'r.pdf', type: 'application/pdf', size: 5 }]);
	});
	it('leaves bad encoded words alone', () => {
		expect(decodeWords('plain')).toBe('plain');
	});
});
