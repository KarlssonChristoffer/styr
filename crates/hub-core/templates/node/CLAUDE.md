# {{name}}

An HTTP service in TypeScript with Hono, on Node. Started from Hub's "Node service" template.

## Build, run and test
- Install: `npm install`
- Run: `npm run dev` (http://localhost:3000, restarts on change; `PORT` changes the port)
- Test: `npm test`
- Type-check: `npm run check`

## Layout
- `src/app.ts`: the Hono app and its routes (`GET /` and `GET /health` to begin with).
- `src/index.ts`: starts the server; keep it free of logic so tests can use `app` directly.
- `src/*.test.ts`: Vitest tests that call `app.request(...)`.

## Rules
- Run the tests and the type-check before calling work done.
- Every new route gets a test.
- Add packages with `npm install <package>`; commit `package-lock.json`.
- Keep secrets in `.env` (git-ignored); never commit them.
