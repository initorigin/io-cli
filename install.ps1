<#
.SYNOPSIS
    Install io on Windows.

.DESCRIPTION
    irm https://raw.githubusercontent.com/initorigin/io-cli/main/install.ps1 | iex

    Downloads the artifact for this machine and the SHA256SUMS beside it,
    VERIFIES THE ARTIFACT BEFORE EXPANDING IT, and installs into
    %LOCALAPPDATA%\io\bin — a directory the current user owns. No administrator
    rights, nothing written outside the user's own profile, and nothing left
    behind if anything fails.

    The checksum defends against a truncated download and a tampered asset. It
    does not defend against a compromised repository; piping a script from the
    internet into a shell is a trust-the-publisher model however it is written.

.PARAMETER Version
    Install this version instead of the latest. Also read from $env:IO_VERSION.

.PARAMETER InstallDir
    Install here instead of %LOCALAPPDATA%\io\bin. Also $env:IO_INSTALL_DIR.

.PARAMETER BaseUrl
    Download from here instead of the GitHub Release. Also $env:IO_BASE_URL.
#>
[CmdletBinding()]
param(
    [string]$Version = $env:IO_VERSION,
    [string]$InstallDir = $env:IO_INSTALL_DIR,
    [string]$BaseUrl = $env:IO_BASE_URL
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repo = 'initorigin/io-cli'

function Fail([string]$message) {
    Write-Error "io install: $message"
    exit 1
}

# Only the one target exists today. Saying so is better than installing an x64
# build on an arm64 machine and letting it fail at run time.
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne 'AMD64') {
    Fail "there is no Windows $arch build yet. Build from source: cargo build --release"
}
$target = 'x86_64-pc-windows-msvc'

if (-not $Version) {
    # The Release page redirects to the newest tag, so the newest version is
    # readable without an API token and without parsing JSON.
    try {
        $response = Invoke-WebRequest -Uri "https://github.com/$repo/releases/latest" -MaximumRedirection 5 -UseBasicParsing
        $final = $response.BaseResponse.RequestMessage.RequestUri.AbsoluteUri
    } catch {
        Fail "could not reach GitHub to find the latest version: $_"
    }
    if ($final -notmatch '/tag/v(?<v>[^/]+)$') {
        Fail "could not work out the latest version from '$final'"
    }
    $Version = $Matches['v']
}

if (-not $BaseUrl) {
    $BaseUrl = "https://github.com/$repo/releases/download/v$Version"
}
if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'io\bin'
}

$stage = "io-$Version-$target"
$archive = "$stage.zip"

# Everything happens in here until it has been verified, so a failure at any
# point leaves the target directory exactly as it was.
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("io-install-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work -Force | Out-Null

try {
    Write-Host "io $Version for $target"

    $archivePath = Join-Path $work $archive
    $sumsPath = Join-Path $work 'SHA256SUMS'
    try {
        Invoke-WebRequest -Uri "$BaseUrl/$archive" -OutFile $archivePath -UseBasicParsing
        Invoke-WebRequest -Uri "$BaseUrl/SHA256SUMS" -OutFile $sumsPath -UseBasicParsing
    } catch {
        Fail "could not download from $BaseUrl : $_"
    }

    $line = Get-Content $sumsPath | Where-Object { $_ -match "\s$([regex]::Escape($archive))$" } | Select-Object -First 1
    if (-not $line) {
        Fail "SHA256SUMS does not mention $archive"
    }
    $expected = ($line -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Path $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()

    if ($expected -ne $actual) {
        Fail "checksum mismatch for $archive`n  expected $expected`n  actual   $actual`nNothing was installed."
    }
    Write-Host 'checksum ok'

    Expand-Archive -Path $archivePath -DestinationPath $work -Force
    $binary = Join-Path $work "$stage\io.exe"
    if (-not (Test-Path $binary)) {
        Fail "$archive does not contain io.exe"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $binary -Destination (Join-Path $InstallDir 'io.exe') -Force
    Write-Host "installed $(Join-Path $InstallDir 'io.exe')"

    # The USER PATH, never the machine PATH: the machine one needs administrator
    # rights and would change the environment for everybody on the box.
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @()
    if ($userPath) { $entries = $userPath -split ';' | Where-Object { $_ } }
    if ($entries -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable('Path', (($entries + $InstallDir) -join ';'), 'User')
        Write-Host ''
        Write-Host "$InstallDir has been added to your user PATH."
        Write-Host 'Open a NEW terminal and run: io'
        Write-Host ''
        Write-Host 'If you would rather set it yourself, this is the line:'
        Write-Host "    `$env:Path += ';$InstallDir'"
    } else {
        Write-Host 'run: io'
    }
} finally {
    Remove-Item -Path $work -Recurse -Force -ErrorAction SilentlyContinue
}
