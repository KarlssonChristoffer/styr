# {{name}}

A plain PHP 8 site with Composer and PHPUnit. Started from Hub's "PHP site" template.

## Build, run and test
- Install: `composer install`
- Run: `composer start` (the same as `php -S localhost:8000 -t public`)
- Test: `composer test`

## Layout
- `public/index.php`: the front controller; it only passes the request to `App\Site::handle`.
- `src/`: the code, autoloaded as the `App\` namespace (PSR-4).
- `tests/`: PHPUnit tests; `Site::handle` is pure so tests need no server.

## Rules
- Run the tests before calling work done.
- Every new route gets a test.
- Escape everything that goes into HTML (`htmlspecialchars`); use prepared statements for any database.
- Add packages with `composer require`, and keep `vendor/` and `.env` out of git.
