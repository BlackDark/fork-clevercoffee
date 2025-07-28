import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	// Consult https://svelte.dev/docs/kit/integrations
	// for more information about preprocessors
	preprocess: vitePreprocess(),

	kit: {
		// Use static adapter for SPA mode
		adapter: adapter({
			// Options for SPA mode
			fallback: 'index.html', // This is crucial for SPA routing
			precompress: false,
			strict: false
		}),

		// adapter: adapter({
		// 	fallback: 'index.html',
		// 	bundleStrategy: 'single'
		// }),

		// output: {
		// 	bundleStrategy: 'single'
		// },

		alias: {
			'@/*': './path/to/lib/*'
		},

		// Paths configuration
		paths: {
			base: process.env.BASE_PATH || '',
			assets: process.env.ASSETS_PATH || ''
		},

		// Ensure proper environment variables are available
		env: {
			publicPrefix: 'PUBLIC_'
		}
	}
};

export default config;
