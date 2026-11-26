# Installs Hub for the current user. Run it once; after that Hub keeps itself up to date.
#
#   powershell -ExecutionPolicy Bypass -File scripts\install.ps1
#
# The updater fetches with your normal git credentials, so the repository may be private.
#
# A new PC takes the ready-made Hub from a PC that already has it, in about a minute, instead of
# compiling it (10-25 minutes). Make a pairing code in Settings > Machines on that PC, then:
#
#   powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -From http://pc:7878 -Code ABCD-EFGH
#
# If that PC cannot be reached or has no build for this kind of PC, this builds from source as
# before (needs git, cargo and npm).
param(
    [string]$Repo = "https://github.com/KarlssonChristoffer/hub",
    [string]$Branch = "main",
    [string]$From = "",
    [string]$Code = ""
)
$ErrorActionPreference = "Stop"

$root = Join-Path $env:LOCALAPPDATA "hub"
$src = Join-Path $root "src"

# The ready-made Hub of the PC at $From, put in `bin\<version>` as the updater would have. True
# when it is installed; any problem is said and left to the build from source.
function Install-Prebuilt {
    $ProgressPreference = "SilentlyContinue"
    $partial = ""
    try {
        $arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
        $base = $From.TrimEnd("/")
        $paired = Invoke-RestMethod -Method Post -Uri "$base/api/pair/redeem" -ContentType "application/json" `
            -Body (@{ code = $Code; name = $env:COMPUTERNAME } | ConvertTo-Json)
        $zip = Join-Path $env:TEMP "hub-prebuilt.tar.gz"
        Invoke-WebRequest -Uri "$base/api/dist?platform=windows-$arch" -Headers @{ Authorization = "Bearer $($paired.token)" } -OutFile $zip
        $partial = Join-Path $root "bin\prebuilt.partial"
        Remove-Item -Recurse -Force $partial -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Force $partial | Out-Null
        tar -xzf $zip -C $partial
        if ($LASTEXITCODE -ne 0) { throw "could not unpack the download" }
        Remove-Item -Force $zip -ErrorAction SilentlyContinue
        foreach ($f in "hubd.exe", "hub-app.exe", "hub-launcher.exe", "VERSION") {
            if (-not (Test-Path (Join-Path $partial $f))) { throw "the download has no $f" }
        }
        $version = (Get-Content (Join-Path $partial "VERSION") -Raw).Trim()
        $dest = Join-Path $root "bin\$version"
        Remove-Item -Recurse -Force $dest -ErrorAction SilentlyContinue
        Move-Item $partial $dest
        Copy-Item (Join-Path $dest "hub-launcher.exe") (Join-Path $root "Hub.exe") -Force
        Set-Content -NoNewline -Path (Join-Path $root "current.tmp") -Value $version
        Move-Item -Force (Join-Path $root "current.tmp") (Join-Path $root "current")
        Write-Host "Installed Hub $version from $From."
        return $true
    } catch {
        Write-Host "Could not take Hub from $From ($($_.Exception.Message)). Building it from source instead."
        if ($partial) { Remove-Item -Recurse -Force $partial -ErrorAction SilentlyContinue }
        return $false
    }
}

# A core started from a development checkout holds the port.
Get-Process hubd -ErrorAction SilentlyContinue | Stop-Process -Force

$ready = $false
if ($From -and $Code) {
    $ready = Install-Prebuilt
} elseif ($From -or $Code) {
    Write-Host "-From and -Code go together; building from source."
}

$haveGit = [bool](Get-Command git -ErrorAction SilentlyContinue)
if (-not $ready) {
    foreach ($tool in "git", "cargo", "npm") {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            throw "$tool is missing. Install it and run this again."
        }
    }
}

# The updater's own clone. Never work in it; it is reset to the branch on every update.
if (-not $ready -or $haveGit) {
    if (Test-Path (Join-Path $src ".git")) {
        git -C $src fetch --quiet origin $Branch
        git -C $src reset --quiet --hard "origin/$Branch"
    } elseif ($ready) {
        # Only for the updater; a clone that cannot be made (no GitHub login yet) is not fatal.
        git clone --quiet --branch $Branch $Repo $src
        if ($LASTEXITCODE -ne 0) { Write-Host "Could not clone $Repo; Hub will not update itself until git can." }
    } else {
        git clone --quiet --branch $Branch $Repo $src
    }
}

if (-not $ready) {
    # Build the installer in its own target folder, so it never locks the binaries it builds.
    Push-Location $src
    try {
        cargo run --quiet --release --target-dir (Join-Path $root "bootstrap") -p hubd -- install
        if ($LASTEXITCODE -ne 0) { throw "Building Hub failed; see $root\update.log" }
    } finally {
        Pop-Location
    }
}

# Start menu shortcut to the launcher, which always starts the newest installed version.
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath("Programs")) "Hub.lnk"))
$link.TargetPath = Join-Path $root "Hub.exe"
$icon = Join-Path $src "app\src-tauri\icons\icon.ico"
if (Test-Path $icon) { $link.IconLocation = $icon }
$link.Save()
if ($ready) {
    # The source build starts Hub itself; a ready-made one is started here.
    Start-Process (Join-Path $root "Hub.exe")
}

Write-Host "Hub is installed. Start it from the Start menu; it updates itself from $Branch."
