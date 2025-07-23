import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],

	// Define environment variables
	define: {
		'process.env.NODE_ENV': JSON.stringify(process.env.NODE_ENV || 'development'),
		'process.env.BASE_PATH': JSON.stringify(process.env.BASE_PATH || '')
	},

	build: {
		cssCodeSplit: false,
		rollupOptions: {
			output: {
				// Force everything into a single chunk
				manualChunks: () => 'everything.js'
			}
		}
	},

	// Build configuration
	// build: {
	// 	// Ensure proper asset handling
	// 	assetsInlineLimit: 4096,
	// 	rollupOptions: {
	// 		output: {
	// 			manualChunks: {
	// 				// Split vendor code for better caching
	// 				vendor: ['lucide-svelte']
	// 			}
	// 		}
	// 	}
	// },

	// Server configuration
	server: {
		fs: {
			// Allow serving files from one level up to the project root
			allow: ['..']
		}
	},

	// Resolve configuration
	resolve: {
		// Ensure proper alias resolution
		alias: {
			$lib: '/src/lib',
			$app: '/.svelte-kit/runtime/app'
		}
	},

	test: {
		projects: [
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
