# {{name}}

A plain PHP site: a front controller, the built-in dev server and PHPUnit.

```
composer install   # install
composer start     # run on http://localhost:8000
composer test      # test
```

Requests are answered in `src/Site.php`; `public/index.php` only hands them over.
