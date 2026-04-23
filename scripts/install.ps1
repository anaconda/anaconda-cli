<#
.SYNOPSIS
    Installer script for ana.

.DESCRIPTION
    Install the ana CLI tool.

    Environment variables
      ANA_INSTALL_DIR          Same as -InstallDir
      ANA_VERSION              Same as -Version
      ANA_VERIFY_CHECKSUM      Set to "false" to skip checksum verification
      ANA_NO_PATH_UPDATE       Set to non-empty to skip PATH update
      ANA_BOOTSTRAP            Set to "false" to skip bootstrap
      ANA_FORCE_INSTALL        Set to non-empty to overwrite without prompting
      GITHUB_TOKEN             Same as -Token

.PARAMETER InstallDir
    Installation directory (default: ${env:USERPROFILE}\.local\bin (Windows) or ${env:HOME}/.local/bin).

.PARAMETER Version
    Version to installi (default: latest).

.PARAMETER Force
    Overwrite existing installation without prompting.

.PARAMETER NoVerifyChecksum
    Disable checksum validation after download (default: false).

.PARAMETER NoPathUpdate
    Skip shell profile modification.

.PARAMETER NoBootstrap
    Skip running 'ana bootstrap' after installation.

.PARAMETER Token
    GitHub token for private repo access.

.EXAMPLE
    PS> & .\setup.ps1

    Installs ana and bootstraps anaconda-cli.

.EXAMPLE
    PS> & .\setup.ps1 -Version '0.0.9'

    Installs a specific version

.EXAMPLE
    PS> & .\setup.ps1 -Force

    Overwrites an existing installation without prompting..

.EXAMPLE
    PS> & .\setup.ps1 -NoBootstrap -NoPathUpdate

    Installs without bootstrapping and updating PATH.

.LINK
    https://github.com/anaconda/ana-cli

#>

[CmdletBinding()]
param(
    [switch] $Force,
    [string] $InstallDir = $(if ($env:USERPROFILE) { "${env:USERPROFILE}\.local\bin" } else { "${env:HOME}/.local/bin" }),
    [switch] $NoBootstrap,
    [switch] $NoPathUpdate,
    [switch] $NoVerifyChecksum,
    [string] $Version = "latest"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Repo = "anaconda/ana-cli"
$BinaryName = "ana"

function Get-OS {
    # PowerShell 6+ has $IsWindows, $IsLinux, $IsMacOS
    # PowerShell 5 only runs on Windows and doesn't have these variables
    if ($PSVersionTable.PSVersion.Major -ge 6) {
        if ($IsWindows) { return "Windows" }
        if ($IsLinux) { return "Linux" }
        if ($IsMacOS) { return "macOS" }
        throw "Unsupported operating system"
    } else {
        # PowerShell 5.x is Windows-only
        return "Windows"
    }
}

function Get-Arch {
    $os = Get-OS
    if ($os -eq "Windows") {
        switch ($env:PROCESSOR_ARCHITECTURE) {
            "AMD64" { return "x86_64" }
            default { throw "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
        }
    } else {
        # Linux/macOS - use uname
        $arch = uname -m
        switch ($arch) {
            "x86_64"  { return "x86_64" }
            "amd64"   { return "x86_64" }
            "aarch64" { return "aarch64" }
            "arm64"   { return "aarch64" }
            default   { throw "Unsupported architecture: $arch" }
        }
    }
}

function Get-Target {
    param(
        [string]$OS,
        [string]$Arch
    )

    switch ("$OS-$Arch") {
        "Linux-x86_64"   { return "linux-x86_64" }
        "Linux-aarch64"  { return "linux-aarch64" }
        "macOS-x86_64"   { return "darwin-x86_64" }
        "macOS-aarch64"  { return "darwin-arm64" }
        "Windows-x86_64" { return "windows-x86_64" }
        default          { throw "No prebuilt binary for $OS $Arch" }
    }
}

function Get-AuthHeader {
    $token = $env:GITHUB_TOKEN
    if (-not $token) {
        # Try gh CLI
        try {
            $token = gh auth token 2>$null
        } catch {
            $token = $null
        }
    }

    if ($token) {
        return @{ Authorization = "token $token" }
    }
    return @{}
}

function Resolve-GitHubAssetUrl {
    param(
        [string]$Version,
        [string]$BinaryName,
        [hashtable]$Headers
    )

    if ($Version -eq "latest") {
        $apiUrl = "https://api.github.com/repos/$Repo/releases/tags/latest"
    } else {
        $ver = $Version -replace "^v", ""
        $apiUrl = "https://api.github.com/repos/$Repo/releases/tags/v$ver"
    }

    try {
        $response = Invoke-RestMethod -Uri $apiUrl -Headers $Headers -UseBasicParsing
    } catch {
        throw "Failed to fetch release info from GitHub API for version: $Version"
    }

    $asset = $response.assets | Where-Object { $_.name -eq $BinaryName } | Select-Object -First 1
    if (-not $asset) {
        throw "Asset '$BinaryName' not found in release $Version"
    }

    return $asset.url
}

function Resolve-DownloadUrl {
    param(
        [string]$Version,
        [string]$BinaryName,
        [hashtable]$AuthHeader
    )

    if ($env:ANA_BASE_URL) {
        $url = "$env:ANA_BASE_URL/$BinaryName"
        $checksumUrl = "$url.sha256"
    } elseif ($AuthHeader.Count -gt 0) {
        $url = Resolve-GitHubAssetUrl -Version $Version -BinaryName $BinaryName -Headers $AuthHeader
        try {
            $checksumUrl = Resolve-GitHubAssetUrl `
                -Version $Version `
                -BinaryName "$BinaryName.sha256" `
                -Headers $AuthHeader
        } catch {
            $checksumUrl = $null
        }
    } elseif ($Version -eq "latest") {
        $url = "https://github.com/$Repo/releases/latest/download/$BinaryName"
        $checksumUrl = "$url.sha256"
    } else {
        $ver = $Version -replace "^v", ""
        $url = "https://github.com/$Repo/releases/download/v$ver/$BinaryName"
        $checksumUrl = "$url.sha256"
    }

    return @{
        Url = $url
        ChecksumUrl = $checksumUrl
    }
}

function Invoke-Download {
    param(
        [string]$Url,
        [string]$Destination,
        [string]$ChecksumUrl = $null,
        [bool]$VerifyChecksum = $true,
        [hashtable]$Headers = @{}
    )

    $downloadHeaders = $Headers.Clone()
    if ($downloadHeaders.Count -gt 0) {
        $downloadHeaders["Accept"] = "application/octet-stream"
    }

    try {
        $ProgressPreference = "SilentlyContinue"
        Invoke-WebRequest -Uri $Url -OutFile $Destination -Headers $downloadHeaders -UseBasicParsing
    } catch {
        throw "Download failed: $Url`n$($_.Exception.Message)"
    }

    if (-not (Test-Path $Destination) -or (Get-Item $Destination).Length -eq 0) {
        throw "Downloaded file is empty. Check the URL or try again."
    }

    # Checksum verification
    if (-not $VerifyChecksum) {
        Write-Host "! Checksum verification disabled" -ForegroundColor Yellow
        return
    }

    if (-not $ChecksumUrl) {
        Write-Host "! Checksum file not available, skipping verification" -ForegroundColor Yellow
        return
    }

    Write-Host "> Verifying checksum" -ForegroundColor Green

    $checksumFile = [System.IO.Path]::GetTempFileName()
    try {
        Invoke-WebRequest -Uri $ChecksumUrl -OutFile $checksumFile -Headers $downloadHeaders -UseBasicParsing
    } catch {
        Write-Host "! Checksum file not available, skipping verification" -ForegroundColor Yellow
        Remove-Item -Path $checksumFile -ErrorAction SilentlyContinue
        return
    }

    $expected = (Get-Content $checksumFile -Raw).Trim().Split()[0]
    Remove-Item -Path $checksumFile -ErrorAction SilentlyContinue

    $actual = (Get-FileHash -Path $Destination -Algorithm SHA256).Hash.ToLower()

    if ($expected -ne $actual) {
        throw "Checksum mismatch!`n  expected: $expected`n  actual:   $actual"
    }

    Write-Host "> Checksum OK" -ForegroundColor Green
}

function Install-Binary {
    param(
        [string]$Source,
        [string]$AnaBin,
        [bool]$Force = $false
    )

    if ((Test-Path $AnaBin) -and -not $Force) {
        Write-Host "  $AnaBin already exists. Overwrite? [y/N] " -NoNewline
        $reply = Read-Host
        if ($reply -notmatch "^[Yy]") {
            throw "Installation cancelled."
        }
    }

    $installDir = Split-Path -Parent $AnaBin
    if (-not (Test-Path $installDir)) {
        New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    }

    Move-Item -Path $Source -Destination $AnaBin -Force
    Write-Host "> Installed ana to $AnaBin" -ForegroundColor Green
}

function Update-Path {
    param(
        [string]$InstallDir
    )

    Write-Host "> Adding ana installation to PATH" -ForegroundColor Green
    $os = Get-OS

    if ($os -eq "Windows") {
        # Must use pipe for PowerShell 5 compatibility
        $anaBinDir = Join-Path -Path $env:USERPROFILE ".ana" | Join-Path -ChildPath "bin"
        Add-ToUserPath -Directory $InstallDir
        Add-ToUserPath -Directory $anaBinDir
    } else {
        $anaBinDir = Join-Path $env:HOME ".ana" "bin"
        Add-ToShellProfile -Directory $InstallDir
        Add-ToShellProfile -Directory $anaBinDir
    }
}

function Add-ToUserPath {
    param(
        [string]$Directory
    )

    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $pathDirs = $currentPath -split ";"

    if ($pathDirs -contains $Directory) {
        Write-Host "$Directory is already in PATH"
        return
    }
    Write-Host "Setting envvar"

    $newPath = "$Directory;$currentPath"
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "> Added $Directory to user PATH" -ForegroundColor Green
    Write-Host "  Restart your terminal for changes to take effect." -ForegroundColor Cyan
}

function Add-ToShellProfile {
    param(
        [string]$Directory
    )

    if ($env:PATH -split ":" -contains $Directory) {
        return
    }

    $line = "export PATH=`"$Directory`:`$PATH`""
    $shell = Split-Path -Leaf $env:SHELL

    switch ($shell) {
        "bash" { $profilePath = Join-Path $env:HOME ".bashrc" }
        "zsh"  { $profilePath = Join-Path $env:HOME ".zshrc" }
        "fish" {
            $line = "set -gx PATH `"$Directory`" `$PATH"
            $profilePath = Join-Path $env:HOME ".config" "fish" "config.fish"
        }
        default {
            Write-Host "! $Directory is not in your PATH." -ForegroundColor Yellow
            Write-Host "  Add it with: $line" -ForegroundColor Yellow
            return
        }
    }

    if ( `
        (Test-Path $profilePath) `
        -and (Get-Content $profilePath -Raw) -match [regex]::Escape($line) `
    ) {
        return
    }

    Add-Content -Path $profilePath -Value "`n$line"
    Write-Host `
        "> Updated $profilePath - restart your shell or run: source $profilePath"`
        -ForegroundColor Green
}

function Invoke-Bootstrap {
    param(
        [string]$AnaBin
    )

    Write-Host "> Running ana bootstrap..." -ForegroundColor Green
    try {
        & $AnaBin bootstrap
        Write-Host "> Bootstrap completed successfully" -ForegroundColor Green
    } catch {
        Write-Host `
            "! Bootstrap failed. You can run 'ana bootstrap' manually later." `
            -ForegroundColor Yellow
    }
}

function Main {
    $os = Get-OS
    $arch = Get-Arch
    $target = Get-Target -OS $os -Arch $arch

    if ($env:ANA_VERSION) {$Version = $env:ANA_VERSION }
    if ($env:ANA_INSTALL_DIR) {$InstallDir = $env:ANA_INSTALL_DIR }

    $exeSuffix = if ($os -eq "Windows") { ".exe" } else { "" }
    $binaryName = "ana-$target$exeSuffix"

    $authHeader = Get-AuthHeader
    $urls = Resolve-DownloadUrl `
        -Version $version `
        -BinaryName $binaryName `
        -AuthHeader $authHeader

    Write-Host "> Installing ana for $os $arch" -ForegroundColor Green
    Write-Host "> Downloading $($urls.Url)" -ForegroundColor Green

    $VerifyChecksum = if ($env:ANA_VERIFY_CHECKSUM -eq "false") {
        $false
    } else {
        -not $NoVerifyChecksum
    }

    $tempFile = [System.IO.Path]::GetTempFileName()
    try {
        Invoke-Download `
            -Url $urls.Url `
            -Destination $tempFile `
            -ChecksumUrl $urls.ChecksumUrl `
            -VerifyChecksum $VerifyChecksum `
            -Headers $authHeader

        $forceInstall = $Force -or $env:ANA_FORCE_INSTALL
        $anaBin = Join-Path -Path $InstallDir "ana$exeSuffix"
        Install-Binary `
            -Source $tempFile `
            -AnaBin $anaBin `
            -Force $forceInstall
    } finally {
        Remove-Item -Path $tempFile -ErrorAction SilentlyContinue
    }

    $updatePath = -not $NoPathUpdate -and -not $env:ANA_NO_PATH_UPDATE
    if ($updatePath) {
        Update-Path -InstallDir $InstallDir
    }

    $runBootstrap = if ($env:ANA_BOOTSTRAP -eq "false") {
        $false
    } else {
        -not $NoBootstrap
    }
    if ($runBootstrap) {
        Invoke-Bootstrap -AnaBin $anaBin
    }

    Write-Host ""
    Write-Host "Done! Run 'ana --help' to get started." -ForegroundColor Cyan
}

Main
