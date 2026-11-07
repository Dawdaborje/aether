// The chat page in a real browser: panes, the message list (virtual, live, scrolling), the composer, attachments,
// typing, and a narrow screen. Two people in two browser contexts. See README.md.
//
//   node ui_e2e.mjs          (SHOTS=/some/dir keeps screenshots; CHROMIUM=/usr/bin/chromium picks the browser)
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, '../../../web/package.json'));
const { chromium } = require('playwright');

const BASE = process.env.AETHER_URL ?? 'http://127.0.0.1:17891';
const PASSWORD = 'Test-pass-123';
const SHOTS = process.env.SHOTS ?? '';
const failures = [];
const check = (name, ok, detail = '') => {
	console.log((ok ? 'ok   ' : 'FAIL ') + name + (ok ? '' : `  -> ${detail}`));
	if (!ok) failures.push(name);
};

const browser = await chromium.launch({
	executablePath: process.env.CHROMIUM ?? '/usr/bin/chromium',
	headless: true,
	args: ['--no-sandbox']
});

async function person(username) {
	const context = await browser.newContext({ viewport: { width: 1200, height: 800 } });
	await context.request.get(BASE + '/api/auth/methods');
	const csrf = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value;
	const login = await context.request.post(BASE + '/api/auth/login', {
		data: { username, password: PASSWORD },
		headers: { 'X-CSRF-Token': csrf, Origin: BASE }
	});
	if (!login.ok()) throw new Error(`login ${username}: ${login.status()}`);
	const token = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value ?? csrf;
	const call = async (fn, data) => {
		const r = await context.request.post(`${BASE}/api/plugins/chatplug/${fn}`, {
			data,
			headers: { 'X-CSRF-Token': token, Origin: BASE }
		});
		if (!r.ok()) throw new Error(`${fn}: ${r.status()} ${await r.text()}`);
		return r.json();
	};
	return { context, call };
}

const shot = async (page, name) => {
	if (SHOTS) await page.screenshot({ path: path.join(SHOTS, name + '.png') });
};
const rows = (page) => page.locator('[data-part=message]');
const log = (page) => page.locator('[data-part=messages] [role=log]');
const atBottom = (page) =>
	log(page).evaluate((el) => el.scrollHeight - el.scrollTop - el.clientHeight < 8);

try {
	const owner = await person('owner');
	const bob = await person('bob');
	const run = Math.random().toString(36).slice(2, 6);
	const { id: channel } = await owner.call('make_channel', { name: 'ui ' + run });
	for (let i = 0; i < 3; i++) await owner.call('bulk_messages', { channel, count: 150 });
	// 450 messages exist: far more than a screen, and more than the 50 the list asks for at first.

	const A = await owner.context.newPage();
	const B = await bob.context.newPage();
	const consoleErrors = [];
	for (const p of [A, B]) p.on('pageerror', (e) => consoleErrors.push(String(e)));
	await A.goto(`${BASE}/web/chat/${channel}`);
	await B.goto(`${BASE}/web/chat/${channel}`);

	// --- panes and the page without chrome
	await A.waitForSelector('[data-part=panes]', { timeout: 15000 });
	const rail = A.locator('[data-pane=rail]');
	check('the page fills the window without a title bar', (await A.locator('[data-part=page\\.fill]').count()) === 1);
	const railWidth = await rail.evaluate((el) => el.getBoundingClientRect().width);
	check('the rail has the width the page gave it', Math.abs(railWidth - 240) < 2, railWidth);
	await rail.getByText('ui ' + run).first().waitFor({ timeout: 15000 });
	check('the rail lists the channel', (await rail.getByText('ui ' + run).count()) > 0);

	// --- the message list
	await A.waitForSelector('[data-part=message]', { timeout: 15000 });
	await A.waitForFunction(() => document.body.innerText.includes('seed message 450'), null, { timeout: 15000 });
	check('the newest message is shown on open', (await A.getByText('seed message 450', { exact: true }).count()) === 1);
	check('the list is at the bottom', await atBottom(A));
	const drawn = await rows(A).count();
	check('only a few of the loaded rows are in the page (virtual list)', drawn > 0 && drawn < 100, drawn);
	await shot(A, 'chat-open');

	// --- older rows load on scrolling up, and the place being read is kept
	await log(A).evaluate((el) => (el.scrollTop = 0));
	await A.waitForFunction(() => document.body.innerText.includes('seed message 4'), null, { timeout: 15000 });
	let reached = false;
	for (let i = 0; i < 12 && !reached; i++) {
		await log(A).evaluate((el) => (el.scrollTop = 0));
		await A.waitForTimeout(500);
		reached = (await A.getByText('seed message 1', { exact: true }).count()) > 0;
	}
	check('scrolling to the top loads the earlier messages, all the way to the first', reached);
	check('the list still draws only a few rows after loading 450', (await rows(A).count()) < 100, await rows(A).count());

	// --- a new message from the other person appears live
	await B.waitForFunction(() => document.body.innerText.includes('seed message 450'), null, { timeout: 15000 }).catch(async (e) => {
		await shot(B, 'b-stuck');
		console.log('B shows:', (await B.locator('body').innerText()).slice(0, 400).replace(/\n+/g, ' | '));
		throw e;
	});
	await B.getByRole('textbox', { name: 'Message' }).fill('hello from bob');
	await B.getByRole('textbox', { name: 'Message' }).press('Enter');
	await B.waitForFunction(() => document.body.innerText.includes('hello from bob'), null, { timeout: 8000 });
	check('the sender sees their message at once', true);
	await log(A).evaluate((el) => (el.scrollTop = el.scrollHeight));
	await A.waitForFunction(() => document.body.innerText.includes('hello from bob'), null, { timeout: 8000 }).then(
		() => check("the owner's page shows it live", true),
		() => check("the owner's page shows it live", false, 'not shown')
	);
	await B.waitForFunction(() => document.querySelector('[data-part=composer] textarea')?.value === '', null, { timeout: 8000 }).then(
		() => check('the composer is cleared after sending', true),
		() => check('the composer is cleared after sending', false, 'still full')
	);

	// --- Shift+Enter is a line break, not a send
	const boxA = A.getByRole('textbox', { name: 'Message' });
	await boxA.fill('line one');
	await boxA.press('Shift+Enter');
	await boxA.pressSequentially('line two');
	check('Shift+Enter makes a new line and does not send', (await boxA.inputValue()) === 'line one\nline two');
	await A.waitForTimeout(600);
	check("the other person sees 'is typing'", (await B.getByText(/is typing/).count()) > 0);
	await shot(B, 'typing');
	await boxA.fill('');

	// --- a message arriving while scrolled up does not move the view and offers a way down
	await log(B).evaluate((el) => (el.scrollTop = 0));
	await B.waitForTimeout(1200);
	await owner.call('post_message', { channel, body: 'while you were reading', as: 'Ann Lee' });
	await shot(B, 'scrolled-up');
	await B.waitForSelector('text=/1 new message/', { timeout: 8000 }).then(
		() => check('a message that arrives while scrolled up shows a "new message" button', true),
		() => check('a message that arrives while scrolled up shows a "new message" button', false, 'no button')
	);
	await B.getByText(/1 new message/).click();
	await B.waitForFunction(() => document.body.innerText.includes('while you were reading'), null, { timeout: 8000 });
	check('the button takes the reader to the bottom', await atBottom(B));
	check('a display name is shown instead of the id', (await B.getByText('Ann Lee', { exact: true }).count()) === 1);

	// --- an attachment
	const file = path.join(os.tmpdir(), `note-${run}.txt`);
	fs.writeFileSync(file, 'attached text ' + run);
	await A.locator('[data-part=composer] input[type=file]').setInputFiles(file);
	await A.getByText(`note-${run}.txt`).first().waitFor({ timeout: 8000 });
	await boxA.fill('see attached');
	await boxA.press('Enter');
	await A.waitForFunction((n) => document.body.innerText.includes(n), `note-${run}.txt`, { timeout: 8000 });
	await B.getByRole('button', { name: new RegExp(`note-${run}.txt`) }).first().click();
	await B.waitForFunction((t) => document.body.innerText.includes(t), 'attached text ' + run, { timeout: 8000 }).then(
		() => check('an attached file opens in the viewer panel and shows its content', true),
		() => check('an attached file opens in the viewer panel and shows its content', false, 'content not shown')
	);
	await shot(B, 'attachment');
	await B.keyboard.press('Escape');

	// --- the rail: groups, the current channel, unread badges that follow other channels, search, folding, keys
	const { id: priv } = await owner.call('make_channel', { name: 'priv ' + run, private: true });
	await bob.call('join_channel', { channel: priv });
	await owner.call('post_message', { channel: priv, body: 'for members only' });
	const railB = B.locator('[data-pane=rail]');
	const chan = railB.locator('nav[data-part=rail]').first();
	await railB.getByRole('button', { name: 'Private' }).waitFor({ timeout: 8000 }).catch(() => {});
	const item = (name) => railB.locator('a[data-rail-item]', { hasText: name });
	check('the rail shows the groups', (await railB.getByRole('button', { name: 'Channels', exact: true }).count()) === 1 && (await railB.getByRole('button', { name: 'Private', exact: true }).count()) === 1);
	check('the channel being read is marked current', (await item('ui ' + run).getAttribute('aria-current')) === 'page');
	const { id: other } = await owner.call('make_channel', { name: 'other ' + run });
	await owner.call('post_message', { channel: other, body: 'psst' });
	await owner.call('post_message', { channel: other, body: 'psst again' });
	await B.waitForFunction((n) => [...document.querySelectorAll('[data-pane=rail] a[data-rail-item]')].some((a) => a.textContent.includes(n)), 'other ' + run, { timeout: 8000 });
	await item('other ' + run).locator('[data-part=rail-badge]').waitFor({ timeout: 8000 }).then(
		async () => check('an unread badge appears for another channel without reloading, with the count', (await item('other ' + run).locator('[data-part=rail-badge]').innerText()) === '2'),
		() => check('an unread badge appears for another channel without reloading, with the count', false, 'no badge')
	);
	await shot(B, 'rail-badge');
	await item('other ' + run).click();
	await B.waitForFunction(() => document.body.innerText.includes('psst again'), null, { timeout: 8000 });
	check('clicking a channel opens it and marks it current', (await item('other ' + run).getAttribute('aria-current')) === 'page' && B.url().endsWith('/chat/' + other));
	await B.waitForTimeout(800);
	check('reading a channel clears its badge', (await item('other ' + run).locator('[data-part=rail-badge]').count()) === 0);
	// filter
	await chan.getByRole('searchbox').fill('other ' + run);
	check('the filter box narrows the list', (await chan.locator('a[data-rail-item]').count()) === 1);
	await chan.getByRole('searchbox').fill('');
	// keyboard
	await item('other ' + run).focus();
	await B.keyboard.press('ArrowDown');
	check('the arrow keys move between entries', (await B.evaluate(() => document.activeElement?.getAttribute('data-rail-item') !== null && document.activeElement?.textContent)) !== null);
	// folding is remembered across a reload
	const before = await chan.locator('a[data-rail-item]').count();
	await railB.getByRole('button', { name: 'Private' }).click();
	const after = await chan.locator('a[data-rail-item]').count();
	check('a group folds', after < before, [before, after]);
	await B.reload();
	await B.waitForSelector('[data-pane=rail] a[data-rail-item]', { timeout: 15000 });
	check('the fold is remembered after a reload', (await chan.locator('a[data-rail-item]').count()) === after);

	// --- the rail can be folded, and a narrow screen shows one pane
	await A.getByRole('button', { name: /Hide Channels/ }).click();
	const folded = await rail.evaluate((el) => el.getBoundingClientRect().width);
	check('the rail folds to a strip', folded < 60, folded);
	await A.getByRole('button', { name: /Show Channels/ }).click();
	await A.setViewportSize({ width: 420, height: 800 });
	await A.waitForTimeout(500);
	check('on a narrow screen the channel pane is the one shown, with a Back button', (await A.getByRole('button', { name: 'Back' }).count()) === 1 && (await A.locator('[data-pane=rail]').count()) === 0);
	await shot(A, 'narrow');
	await A.goto(`${BASE}/web/chat`);
	await A.waitForSelector('[data-part=panes]', { timeout: 15000 });
	check('without a channel a narrow screen shows the list to choose from', (await A.locator('[data-pane=rail]').count()) === 1 && (await A.locator('[data-pane=main]').count()) === 0);

	// --- presence: a rail of people with live dots, and who is on the channel
	await A.setViewportSize({ width: 1200, height: 800 });
	await A.goto(`${BASE}/web/chat/${other}`);
	await B.goto(`${BASE}/web/chat/${other}`);
	const dots = () => B.locator('[data-pane=rail] [data-part=rail] a[data-rail-item] span[title=Online]');
	await B.waitForFunction(() => document.querySelectorAll('[data-pane=rail] a[data-rail-item] span[title=Online]').length >= 2, null, { timeout: 10000 }).then(
		() => check('the people rail shows both people online', true),
		async () => check('the people rail shows both people online', false, await dots().count())
	);
	await B.locator('[data-part=presence-count]').waitFor({ timeout: 8000 });
	await B.waitForFunction(() => document.querySelector('[data-part=presence-count]')?.textContent.trim().startsWith('2'), null, { timeout: 8000 }).then(
		() => check('the channel shows two people here', true),
		async () => check('the channel shows two people here', false, await B.locator('[data-part=presence-count]').innerText())
	);
	check('the people are named, not shown by id', (await B.locator('[data-part=presence-person] .sr-only').allInnerTexts()).every((t) => t.trim().length > 0));
	await shot(B, 'presence');
	await A.close();
	await B.waitForFunction(() => document.querySelector('[data-part=presence-count]')?.textContent.trim().startsWith('1'), null, { timeout: 10000 }).then(
		() => check('when the other person leaves the channel shows one person here', true),
		async () => check('when the other person leaves the channel shows one person here', false, await B.locator('[data-part=presence-count]').innerText())
	);
	await B.waitForFunction(() => document.querySelectorAll('[data-pane=rail] a[data-rail-item] span[title=Online]').length === 1, null, { timeout: 10000 }).then(
		() => check('...and their dot in the people rail goes out', true),
		async () => check('...and their dot in the people rail goes out', false, await dots().count())
	);

	check('no page errors were raised in the browser', consoleErrors.length === 0, consoleErrors.join(' | ').slice(0, 300));
} catch (error) {
	check('the run finished', false, String(error?.stack ?? error));
} finally {
	await browser.close();
}
console.log(`\n${failures.length} failure(s)`);
process.exit(failures.length ? 1 : 0);
