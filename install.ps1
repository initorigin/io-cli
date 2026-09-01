<#
.SYNOPSIS
    Install io on Windows.

.DESCRIPTION
    irm https://raw.githubusercontent.com/initorigin/io-cli/main/install.ps1 | iex

    Downloads the artifact for this machine and the SHA256SUMS beside it,
    VERIFIES THE ARTIFACT BEFORE EXPANDING IT, and installs into
    %LOCALAPPDATA%\io\bin — a directory the current user owns. No administrator
    rights, nothing written outside that directory — not your PATH either, which
    is printed for you to set rather than set for you — and nothing left behind
    if anything fails.

    The checksum defends against a truncated download and a tampered asset. It
    does not defend against a compromised repository; piping a script from the
    internet into a shell is a trust-the-publisher model however it is written.

    It says all of that out loud while it happens: the target it resolved, where
    the version came from, every URL it fetches, BOTH checksums before it compares
    them, where the binary landed and what that binary says its version is.
    Printing both checksums rather than "checksum ok" is the point — the operator
    can see the comparison instead of being told its result, and an install that
    only announces its own success is exactly the one nobody can audit. Failures
    go through Fail, which writes to the error stream, so a log of what went right
    never buries the one line somebody greps for.

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
Write-Host "detected Windows $arch -> target $target"

if ($Version) {
    $versionFrom = 'IO_VERSION'
} else {
    # The Release page redirects to the newest tag, so the newest version is
    # readable without an API token and without parsing JSON.
    $versionFrom = "https://github.com/$repo/releases/latest"
    try {
        $response = Invoke-WebRequest -Uri $versionFrom -MaximumRedirection 5 -UseBasicParsing
        $final = $response.BaseResponse.RequestMessage.RequestUri.AbsoluteUri
    } catch {
        Fail "could not reach GitHub to find the latest version: $_"
    }
    if ($final -notmatch '/tag/v(?<v>[^/]+)$') {
        Fail "could not work out the latest version from '$final'"
    }
    $Version = $Matches['v']
}

# Where the version came from decides which of two very different things just
# happened: an operator pinning a version, or this script trusting GitHub to name
# the newest one. Printing the number without its source hides that.
Write-Host "version $Version (from $versionFrom)"

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
    $archivePath = Join-Path $work $archive
    $sumsPath = Join-Path $work 'SHA256SUMS'
    try {
        # Announced before the fetch rather than after it, so a download that
        # hangs or fails names the URL that was in flight.
        Write-Host "downloading $BaseUrl/$archive"
        Invoke-WebRequest -Uri "$BaseUrl/$archive" -OutFile $archivePath -UseBasicParsing
        Write-Host "downloading $BaseUrl/SHA256SUMS"
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

    # Both numbers, before the comparison. "checksum ok" on its own is a claim;
    # these two lines are the evidence for it, and they are what makes a
    # wrong-but-matching SHA256SUMS visible to somebody reading the output.
    Write-Host "expected $expected"
    Write-Host "computed $actual"
    if ($expected -ne $actual) {
        Fail "checksum mismatch for $archive`n  expected $expected`n  actual   $actual`nNothing was installed."
    }
    Write-Host 'checksum ok'

    Expand-Archive -Path $archivePath -DestinationPath $work -Force
    $binary = Join-Path $work "$stage\io.exe"
    if (-not (Test-Path $binary)) {
        Fail "$archive does not contain io.exe"
    }
    Write-Host "unpacked $archive"

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $installed = Join-Path $InstallDir 'io.exe'
    Copy-Item -Path $binary -Destination $installed -Force
    Write-Host "installed $installed"

    # The PATH line is printed rather than written. A user PATH write is not a
    # shell profile — it goes to the user's own environment rather than a file
    # they keep — but it is still a change to the machine that the operator did
    # not ask for, made by a script they piped into a shell, and it outlives the
    # install in a place they have no reason to look. The line below is one they
    # can read before they run it. The user PATH is still READ, because which of
    # the two messages is the true one depends on it; and because nothing is
    # un-written either, an install that already put this directory on the user
    # PATH keeps that entry — this script cannot tell its own old edit from one
    # the operator made on purpose, so it takes neither away.
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @()
    if ($userPath) { $entries = $userPath -split ';' | Where-Object { $_ } }
    if ($entries -notcontains $InstallDir) {
        Write-Host ''
        Write-Host "$InstallDir is not on your PATH. Add this to your PowerShell profile:"
        Write-Host ''
        Write-Host "    `$env:Path += ';$InstallDir'"
        Write-Host ''
        Write-Host 'then open a new terminal and run: io'
    } else {
        Write-Host "$InstallDir is on your user PATH; run: io"
    }

    # The last line of the narration is the binary's own, not this script's: the
    # only proof that what was verified and copied is a program this machine can
    # run. A native command's exit code does not throw, so it is checked by hand.
    & $installed --version
    if ($LASTEXITCODE -ne 0) {
        Fail "installed $installed but it will not run here"
    }
} finally {
    Remove-Item -Path $work -Recurse -Force -ErrorAction SilentlyContinue
}
