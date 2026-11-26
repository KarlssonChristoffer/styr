<?php

declare(strict_types=1);

// The front controller: every request that is not a file in public/ lands here.
require __DIR__ . '/../vendor/autoload.php';

$path = parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH) ?: '/';
$response = App\Site::handle($_SERVER['REQUEST_METHOD'] ?? 'GET', $path);

http_response_code($response['status']);
header('Content-Type: ' . $response['type']);
echo $response['body'];
