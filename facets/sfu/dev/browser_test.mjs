// Two real browsers (headless Chromium with fake camera and microphone) in one room of the dev server.
//   node dev/browser_test.mjs        (starts `cargo run --example dev_server` itself; CHROMIUM=/usr/bin/chromium)
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, '../../../web/package.json'));
const { chromium } = require('playwright');
const BASE = 'http://127.0.0.1:17890';
const failures = [];
const check = (name, ok, detail = '') => {
	console.log((ok ? 'ok   ' : 'FAIL ') + name + (ok ? '' : `  -> ${JSON.stringify(detail)}`));
	if (!ok) failures.push(name);
};
const until = async (fn, ms = 15000) => {
	const end = Date.now() + ms;
	for (;;) {
		const v = await fn();
		if (v) return v;
		if (Date.now() > end) return false;
		await new Promise((r) => setTimeout(r, 250));
	}
};

// The page runs the real client: its TypeScript with the types taken out.
writeFileSync(path.join(here, 'call-client.js'), stripTypeScriptTypes(readFileSync(path.join(here, '../../../web/src/lib/call/client.ts'), 'utf8')));
const server = spawn('cargo', ['run', '-q', '-p', 'aether_sfu', '--example', 'dev_server'], { cwd: path.join(here, '..'), stdio: ['ignore', 'pipe', 'inherit'] });
let ready = false;
server.stdout.on('data', (d) => { if (String(d).includes('listening')) ready = true; if (process.env.LOG) process.stdout.write(String(d)); });
const browser = await chromium.launch({
	executablePath: process.env.CHROMIUM ?? '/usr/bin/chromium',
	headless: true,
	args: ['--no-sandbox', '--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream', '--autoplay-policy=no-user-gesture-required']
});
try {
	check('server starts', await until(() => ready, 240000));
	const room = 'r' + Math.random().toString(36).slice(2, 6);
	const open = async (user) => {
		const page = await (await browser.newContext()).newPage();
		await page.addInitScript(() => { window.__pcs = []; const O = window.RTCPeerConnection; window.RTCPeerConnection = function (...a) { const p = new O(...a); window.__pcs.push(p); return p; }; window.RTCPeerConnection.prototype = O.prototype; });
		page.on('pageerror', (e) => console.log('pageerror', user, e.message));
		await page.goto(`${BASE}/?room=${room}&user=${user}`);
		await page.waitForFunction(() => window.received && window.client);
		return page;
	};
	const ann = await open('ann');
	check('ann is live', await until(async () => (await ann.evaluate(() => document.getElementById('state').textContent)) === 'live'));
	const bob = await open('bob');
	check('bob is live', await until(async () => (await bob.evaluate(() => document.getElementById('state').textContent)) === 'live'));

	const sees = async (page, kind) => until(async () => (await page.evaluate(() => window.received()))[kind] > 20);
	check('ann receives bob audio', await sees(ann, 'audio'), await ann.evaluate(() => window.received()));
	check('ann receives bob video', await sees(ann, 'video'), await ann.evaluate(() => window.received()));
	check('bob receives ann audio', await sees(bob, 'audio'), await bob.evaluate(() => window.received()));
	check('bob receives ann video', await sees(bob, 'video'), await bob.evaluate(() => window.received()));
	check('video is decoded into pictures', await until(async () => (await bob.evaluate(() => window.received())).frames > 5), await bob.evaluate(() => window.received()));
	// Simulcast: ann sends three sizes; bob is given the one he asks for, and changes only at a key frame.
	const width = (page) => page.evaluate(async () => { let w = 0; for (const c of window.__pcs) for (const r of (await c.getStats()).values()) if (r.type === 'inbound-rtp' && r.kind === 'video') w = Math.max(w, r.frameWidth ?? 0); return w; });
	if (process.env.DEBUG_STATS) console.log('ann sends', JSON.stringify(await ann.evaluate(async () => { const out = []; for (const c of window.__pcs) for (const r of (await c.getStats()).values()) if (r.type === 'outbound-rtp' && r.kind === 'video') out.push({ rid: r.rid, w: r.frameWidth, frames: r.framesEncoded, active: r.active, limit: r.qualityLimitationReason, bytes: r.bytesSent }); return out; })));
	check('bob is given ann at full size by default', await until(async () => (await width(bob)) === 960, 30000), await width(bob));
	await bob.evaluate(() => window.client.want('ann', 'low'));
	check('and at the small size when he asks for low', await until(async () => { const w = await width(bob); return w > 0 && w <= 320; }, 20000), await width(bob));
	await bob.evaluate(() => window.client.want('ann', 'high'));
	check('and full size again when he asks for high', await until(async () => (await width(bob)) === 960, 20000), await width(bob));
	// A layer the sender stops is not left to freeze: bob moves to the biggest one still being sent, and back when it returns.
	const setLayer = (page, rid, active) => page.evaluate(async ([rid, active]) => { for (const c of window.__pcs) for (const s of c.getSenders()) if (s.track?.kind === 'video') { const p = s.getParameters(); const e = p.encodings.find((x) => x.rid === rid); if (e) { e.active = active; await s.setParameters(p); } } }, [rid, active]);
	await setLayer(ann, 'h', false);
	check('when ann stops the big layer, bob is moved to the next one', await until(async () => { const w = await width(bob); return w > 0 && w <= 480; }, 25000), await width(bob));
	await setLayer(ann, 'h', true);
	check('and back to the big one when it returns', await until(async () => (await width(bob)) === 960, 25000), await width(bob));
	// Bandwidth: a viewer whose connection cannot carry the big picture is given the small one, whatever the tile asks.
	const limit = (bps) => fetch(`${BASE}/bandwidth?room=${room}&user=bob${bps ? '&bps=' + bps : ''}`);
	await limit(300000);
	check('a viewer short of bandwidth is given the small picture', await until(async () => { const w = await width(bob); return w > 0 && w <= 240; }, 25000), await width(bob));
	await limit(null);
	check('and the big one again when there is room', await until(async () => (await width(bob)) === 960, 25000), await width(bob));
	// The fake microphone beeps, so the server must name it as a speaker, by stream id, to the other side.
	check('a speaker is named to the others', await until(async () => (await ann.evaluate(() => [...window.client.speaking].length)) > 0 || (await bob.evaluate(() => [...window.client.speaking].length)) > 0));
	check('each page names the other in the roster', (await ann.evaluate(() => window.roster())).join() === 'ann,bob', await ann.evaluate(() => window.roster()));

	const carl = await open('carl');
	check('a third person receives both', await until(async () => { const r = await carl.evaluate(() => window.received()); return r.audio > 20 && r.video > 20 && r.tracks >= 4; }), await carl.evaluate(() => window.received()));
	check('and the first two receive carl', await until(async () => (await ann.evaluate(() => window.received())).tracks >= 4), await ann.evaluate(() => window.received()));

	await ann.evaluate(() => window.client.setMuted('audio', true));
	check('muting shows on the others', await until(async () => (await carl.evaluate(() => window.client.roster.find((p) => p.user === 'ann')?.audio)) === false));
	check('and unmuting too', await (async () => { await ann.evaluate(() => window.client.setMuted('audio', false)); return until(async () => (await carl.evaluate(() => window.client.roster.find((p) => p.user === 'ann')?.audio)) === true); })());

	await bob.close();
	check('the roster drops bob', await until(async () => !(await ann.evaluate(() => window.roster().includes('bob')))), await ann.evaluate(() => window.roster()));
	const before = (await ann.evaluate(() => window.received())).video;
	check('carl still reaches ann after bob left', await until(async () => (await ann.evaluate(() => window.received())).video > before + 20), await ann.evaluate(() => window.received()));
	await carl.evaluate(() => window.client.leave());
	check('leaving through the client takes carl off the roster', await until(async () => !(await ann.evaluate(() => window.roster().includes('carl')))), await ann.evaluate(() => window.roster()));
	check('and ends his call', (await carl.evaluate(() => window.client.state)) === 'ended');
} finally {
	await browser.close();
	server.kill();
}
console.log(failures.length ? `\n${failures.length} failed` : '\nall ok');
process.exit(failures.length ? 1 : 0);
