import { fileURLToPath, URL } from "node:url"
import tailwindcss from "@tailwindcss/vite"
import { tanstackRouter } from "@tanstack/router-plugin/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

export default defineConfig({
	plugins: [
		// Must run before the React plugin: it rewrites route files and generates
		// `src/routeTree.gen.ts` on every dev-server start and production build.
		tanstackRouter({
			target: "react",
			autoCodeSplitting: true,
			routesDirectory: "./src/routes",
			generatedRouteTree: "./src/routeTree.gen.ts",
			// Underscore-prefixed folders hold co-located components, data helpers and
			// hooks. They are not routes.
			routeFileIgnorePattern: "^(_apis|_components|_data|_hooks)",
		}),
		react(),
		tailwindcss(),
	],
	resolve: {
		alias: {
			"#": fileURLToPath(new URL("./src", import.meta.url)),
		},
	},
	server: {
		port: 5173,
		strictPort: false,
	},
	build: {
		outDir: "dist",
		sourcemap: true,
	},
})
