# BrewUI frontend

Svelte 5 static SPA with Vite, Tailwind 4, Bits UI dialogs, TanStack Svelte Query,
Zod Mini boundary validation, and uPlot time-series charts. Native form controls
keep keyboard support and avoid an extra form or primitive dependency.

From `ui/`:

```sh
pnpm install --frozen-lockfile
pnpm dev
pnpm check
pnpm --filter @clevercoffee/frontend build
pnpm --filter @clevercoffee/frontend exec playwright install chromium
pnpm --filter @clevercoffee/frontend test:e2e
```

`pnpm check` runs Biome on TypeScript/CSS, Prettier on Svelte markup,
`svelte-check` (including accessibility warnings), and Vitest. TypeScript 7 runs the native checker through `--tsgo`; TypeScript 6
remains installed for Svelte tooling, with TypeScript 7 pinned under the
`@typescript/native` npm alias. Each native check clears its generated cache so
removed or renamed Svelte files cannot leave stale diagnostics.
`pnpm check:types:ts6` provides a compatibility
check using the TypeScript 6 engine. See the [svelte-check documentation](https://www.npmjs.com/package/svelte-check).
ESLint is not used.
Browser tests exercise the bundled mock API; reports and screenshots go to
`playwright-report/` and `test-results/`. Set
`PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` to use an existing Chromium binary.

The development server proxies `/api` and `/events` to the mock server on port
3001. `.env.mock` can also point directly at a remote mock server. Production
uses same-origin `/api` and `/events` without external fonts or CDN resources.

Production assets use `/ui/` and hash navigation (`/ui/#/config/behavior`).
Legacy `/ui/config/behavior` links still initialize the corresponding screen.
The postbuild step retains gzip HTML/JS/CSS and static assets for the existing
firmware asset embedder; the frontend API contract remains unchanged.

TanStack Query owns shared parameter/status requests, command mutations, cache
invalidation, and OTA status polling. Mutations are never automatically retried.
SSE uses the browser's reconnect behavior and closes when the dashboard unmounts.
Charts retain at most 1,200 aligned samples and update existing uPlot instances.
Parameter drafts survive internal navigation, are not overwritten by refetches,
and prompt before a full-page unload. Save requests contain only changed keys
and reject bodies over the device's 1,024-byte parameter limit before sending.
