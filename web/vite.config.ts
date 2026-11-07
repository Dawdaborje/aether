import { paraglideVitePlugin } from '@inlang/paraglide-js';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';
import { playwright } from '@vitest/browser-playwright';
import { sveltekit } from '@sveltejs/kit/vite';

// The app is served under `/web`, but links to its pages (an enrollment invitation) are written without
// it. In development, send such a page request to the real address instead of SvelteKit's error.
const redirectToBase = {
	name: 'redirect-to-base',
	configureServer(server: import('vite').ViteDevServer) {
		server.middlewares.use((req, res, next) => {
			const url = req.url ?? '';
			const wantsPage = req.method === 'GET' && (req.headers.accept ?? '').includes('text/html');
			const outside = !/^\/(web|api|@|node_modules|src|\.svelte-kit)(\/|$|\?)/.test(url) && url !== '/';
			if (wantsPage && outside) {
				res.statusCode = 307;
				res.setHeader('Location', `/web${url}`);
				res.end();
				return;
			}
			next();
		});
	}
};

export default defineConfig({
	plugins: [
		redirectToBase,
		tailwindcss(),
		sveltekit(),
		paraglideVitePlugin({ project: './project.inlang', outdir: './src/lib/paraglide' })
	],
	server: {
		proxy: {
			'/api': {
				target: process.env.PUBLIC_API_BASE_URL || 'http://127.0.0.1:7890',
				changeOrigin: true,
				ws: true
			}
		}
	},
	test: {
		expect: { requireAssertions: true },
		projects: [
			{
				extends: './vite.config.ts',
				test: {
					name: 'client',
					browser: {
						enabled: true,
						provider: playwright(),
						instances: [{ browser: 'chromium', headless: true }]
					},
					include: ['src/**/*.svelte.{test,spec}.{js,ts}'],
					exclude: ['src/lib/server/**']
				}
			},

			{
				extends: './vite.config.ts',
				test: {
					name: 'server',
					environment: 'node',
					include: ['src/**/*.{test,spec}.{js,ts}'],
					exclude: ['src/**/*.svelte.{test,spec}.{js,ts}']
				}
			}
		]
	}
});
