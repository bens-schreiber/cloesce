#Requires -Version 5

$ErrorActionPreference = 'Stop'

$Repo = "bens-schreiber/cloesce"
$BinaryName = "cloesce.exe"

# Detect architecture
$Arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    'AMD64' { 'x86_64' }
    'ARM64' { 'aarch64' }
    default {
        Write-Error "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE"
        exit 1
    }
}

$AssetName = "cloesce-compiler-${Arch}-windows"

Write-Host "Fetching latest Cloesce release..."
try {
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
} catch {
    Write-Error "Failed to fetch release information: $_"
    exit 1
}

$LatestTag = $Release.tag_name
if (-not $LatestTag) {
    Write-Error "Could not determine the latest release tag."
    exit 1
}

Write-Host "Latest release: $LatestTag"

$DownloadUrl = "https://github.com/$Repo/releases/download/$LatestTag/${AssetName}.zip"

# Determine install directory and PATH scope
$IsAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)
if ($IsAdmin) {
    $InstallDir = Join-Path $env:ProgramFiles "cloesce"
    $PathScope = "Machine"
} else {
    $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\cloesce"
    $PathScope = "User"
}

$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $TmpDir | Out-Null

try {
    $ZipPath = Join-Path $TmpDir "${AssetName}.zip"

    Write-Host "Downloading ${AssetName}.zip..."
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing

    Write-Host "Extracting..."
    Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force

    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir | Out-Null
    }

    $SrcBinary = Join-Path $TmpDir $BinaryName
    $DstBinary = Join-Path $InstallDir $BinaryName

    Write-Host "Installing $BinaryName to $InstallDir..."
    Copy-Item -Path $SrcBinary -Destination $DstBinary -Force

} finally {
    Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue
}

# Add install dir to PATH if not already present
$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", $PathScope)
$PathEntries = $CurrentPath -split ';' | Where-Object { $_ -ne '' }

if ($InstallDir -notin $PathEntries) {
    $NewPath = ($PathEntries + $InstallDir) -join ';'
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, $PathScope)
    Write-Host ""
    Write-Host "Added $InstallDir to the $PathScope PATH."
    Write-Host "Restart your terminal for the PATH change to take effect."
}

Write-Host ""
Write-Host "Cloesce $LatestTag installed successfully!"
Write-Host "Run: cloesce version"
