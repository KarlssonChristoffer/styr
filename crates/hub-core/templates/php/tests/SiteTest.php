<?php

declare(strict_types=1);

namespace Tests;

use App\Site;
use PHPUnit\Framework\TestCase;

final class SiteTest extends TestCase
{
    public function testTheFrontPageShowsTheName(): void
    {
        $response = Site::handle('GET', '/');

        $this->assertSame(200, $response['status']);
        $this->assertStringContainsString(Site::NAME, $response['body']);
    }

    public function testHealthIsOk(): void
    {
        $response = Site::handle('GET', '/health');

        $this->assertSame(200, $response['status']);
        $this->assertSame('{"status":"ok"}', $response['body']);
    }

    public function testAnUnknownPathIsNotFound(): void
    {
        $this->assertSame(404, Site::handle('GET', '/nope')['status']);
    }
}
