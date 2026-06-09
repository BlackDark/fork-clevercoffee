# CleverCoffee Frontend

React + Vite UI for CleverCoffee. Linting and formatting use [Biome](https://biomejs.dev/) from the `ui/` workspace root.

## Scripts

From `ui/packages/frontend`:

- `pnpm dev` — dev server (mock API mode)
- `pnpm build` — production build
- `pnpm test:run` — unit tests
- `pnpm tsc` — type check
- `pnpm lint` — Biome check for this package

From `ui/`:

- `pnpm lint` — Biome check for the whole UI monorepo
- `pnpm format` — apply Biome fixes (format, lint, organize imports)
