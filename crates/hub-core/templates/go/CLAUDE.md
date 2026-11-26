# {{name}}

An HTTP service on Go's standard library (`net/http`). Started from Hub's "Go service" template.

## Build, run and test
- Run: `go run .` (http://localhost:8080; `PORT` changes the port)
- Test: `go test ./...`
- Check: `go vet ./...`
- Build: `go build -o bin/ .`

## Layout
- `main.go`: `newMux` holds the routes (`GET /health` to begin with), `main` only starts the server.
- `*_test.go`: tests that call the routes through `httptest`.

## Rules
- Run the tests and `go vet` before calling work done.
- Every new route gets a test. Prefer the standard library; add a dependency only when it earns its place.
- Keep secrets in the environment, never in the repository.
