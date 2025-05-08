import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import Icons from 'unplugin-icons/vite';
import { SvelteKitPWA } from '@vite-pwa/sveltekit';

export default defineConfig({
	plugins: [
		sveltekit(),
		tailwindcss(),
		Icons({
			compiler: 'svelte',
			autoInstall: true,
			// Configure for separate files

			iconCustomizer(collection, icon, props) {
				props.mode = 'url';
			}
		}),
		SvelteKitPWA()
		/*{
			strategies: 'injectManifest',
			srcDir: 'src',
			filename: 'service-worker.js',
			manifest: {
				name: 'Viz.rs',
				short_name: 'Viz.rs',
				theme_color: '#ff3e00',
				icons: [
					{
						src: 'favicon.png',
						sizes: '16x16',
						type: 'image/png'
					},
					{
						src: 'favicon.png',
						sizes: '32x32',
						type: 'image/png'
					},
					{
						src: 'favicon.png',
						sizes: '48x48',
						type: 'image/png'
					}
				]
			}
			// other pwa options 
		} */
	],
	server: {
		allowedHosts: true,
			port: 3011
	}
});
