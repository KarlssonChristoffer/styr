<?php

declare(strict_types=1);

namespace App;

/** Turns a request into a response. Pure, so a test can call it without a server. */
final class Site
{
    public const NAME = '{{name}}';

    /** @return array{status: int, type: string, body: string} */
    public static function handle(string $method, string $path): array
    {
        if ($method === 'GET' && $path === '/') {
            $name = htmlspecialchars(self::NAME, ENT_QUOTES);
            return [
                'status' => 200,
                'type' => 'text/html; charset=utf-8',
                'body' => "<!doctype html><title>{$name}</title><h1>{$name}</h1>",
            ];
        }
        if ($method === 'GET' && $path === '/health') {
            return ['status' => 200, 'type' => 'application/json', 'body' => '{"status":"ok"}'];
        }
        return ['status' => 404, 'type' => 'text/plain; charset=utf-8', 'body' => 'Not found'];
    }
}
