<#
.SYNOPSIS
    Installiert, aktualisiert oder entfernt SlideBear unter Windows.

.DESCRIPTION
    Lädt die neueste Version von GitHub (Releases) herunter, installiert sie nach
    %LOCALAPPDATA%\Programs\SlideBear (keine Admin-Rechte nötig) und legt Verknüpfungen
    im Startmenü und auf dem Desktop an. Erneut ausführen = Update.
    Deine Daten (Termine, Slides, Einstellungen) liegen getrennt unter
    %APPDATA%\ProKode\SlideBear und bleiben bei Update und Deinstallation erhalten.

.EXAMPLE
    # Installieren oder aktualisieren (PowerShell):
    irm https://raw.githubusercontent.com/BKoderisch/SlideBear2/main/scripts/install-windows.ps1 | iex

.EXAMPLE
    # Aus lokaler Datei, ohne Desktop-Verknüpfung:
    powershell -ExecutionPolicy Bypass -File install-windows.ps1 -NoDesktopShortcut

.EXAMPLE
    # Deinstallieren:
    powershell -ExecutionPolicy Bypass -File install-windows.ps1 -Uninstall

.EXAMPLE
    # Selbst bauen statt herunterladen (installiert bei Bedarf Rust über winget):
    powershell -ExecutionPolicy Bypass -File install-windows.ps1 -FromSource
#>
param(
    [switch]$Uninstall,
    [switch]$FromSource,
    [switch]$NoDesktopShortcut
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # macht Invoke-WebRequest in PowerShell 5 deutlich schneller
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo       = 'BKoderisch/SlideBear2'
$AssetName  = 'SlideBear-windows-x64.zip'
$InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\SlideBear'
$Exe        = Join-Path $InstallDir 'SlideBear.exe'
$StartLink  = Join-Path ([Environment]::GetFolderPath('Programs')) 'SlideBear.lnk'
$DeskLink   = Join-Path ([Environment]::GetFolderPath('Desktop')) 'SlideBear.lnk'
$DataDir    = Join-Path $env:APPDATA 'ProKode\SlideBear'

function Write-Step($text) { Write-Host "  > $text" -ForegroundColor Cyan }

function Stop-SlideBear {
    $running = Get-Process -Name 'SlideBear' -ErrorAction SilentlyContinue
    if ($running) {
        Write-Step 'SlideBear läuft noch und wird beendet ...'
        $running | Stop-Process -Force
        Start-Sleep -Seconds 1
    }
}

function New-Shortcut($Path, $Target) {
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($Path)
    $link.TargetPath = $Target
    $link.WorkingDirectory = Split-Path $Target
    $link.IconLocation = "$Target,0"
    $link.Description = 'SlideBear: Veranstaltungs-Slides aus ChurchTools'
    $link.Save()
}

Write-Host ''
Write-Host '  SlideBear für Windows' -ForegroundColor White
Write-Host ''

# ------------------------------------------------------------------------------------------
if ($Uninstall) {
    Stop-SlideBear
    foreach ($p in @($StartLink, $DeskLink)) { if (Test-Path $p) { Remove-Item $p -Force } }
    if (Test-Path $InstallDir) { Remove-Item $InstallDir -Recurse -Force }
    Write-Host '  SlideBear wurde entfernt.' -ForegroundColor Green
    Write-Host "  Deine Daten liegen weiterhin unter $DataDir (bei Bedarf von Hand löschen)."
    return
}

Stop-SlideBear
New-Item -ItemType Directory -Force $InstallDir | Out-Null

# ------------------------------------------------------------------------------------------
if ($FromSource) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Step 'Rust ist nicht installiert, installiere über winget ...'
        winget install --id Rustlang.Rustup -e --accept-source-agreements --accept-package-agreements
        $env:Path = [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + [Environment]::GetEnvironmentVariable('Path', 'Machine')
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            throw 'Rust wurde installiert, ist aber in dieser Sitzung noch nicht verfügbar. Bitte PowerShell neu öffnen und das Skript erneut starten.'
        }
        Write-Host '  Hinweis: Rust braucht unter Windows die "Visual Studio Build Tools" (C++). Falls der Build fehlschlägt:'
        Write-Host '  winget install Microsoft.VisualStudio.2022.BuildTools --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive"'
    }
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        Write-Step 'Git ist nicht installiert, installiere über winget ...'
        winget install --id Git.Git -e --accept-source-agreements --accept-package-agreements
        $env:Path = [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + [Environment]::GetEnvironmentVariable('Path', 'Machine')
    }
    $src = Join-Path $env:LOCALAPPDATA 'SlideBear-src'
    if (Test-Path (Join-Path $src '.git')) {
        Write-Step 'Quellcode aktualisieren ...'
        git -C $src pull --ff-only
    } else {
        Write-Step 'Quellcode herunterladen ...'
        git clone "https://github.com/$Repo.git" $src
    }
    Write-Step 'Bauen (beim ersten Mal einige Minuten) ...'
    Push-Location $src
    try { cargo build --release -p slidebear; if ($LASTEXITCODE -ne 0) { throw 'Build fehlgeschlagen.' } }
    finally { Pop-Location }
    Copy-Item (Join-Path $src 'target\release\slidebear.exe') $Exe -Force
    $version = 'aus dem Quellcode'
}
else {
    Write-Step 'Neueste Version suchen ...'
    $headers = @{ 'User-Agent' = 'SlideBear-Installer'; 'Accept' = 'application/vnd.github+json' }
    try {
        $release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest" -Headers $headers
    } catch {
        throw "Keine Version auf GitHub gefunden ($($_.Exception.Message)). Alternativ mit -FromSource selbst bauen."
    }
    $asset = $release.assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1
    if (-not $asset) { throw "Das Release $($release.tag_name) enthält keine Datei $AssetName." }

    $zip = Join-Path $env:TEMP $AssetName
    Write-Step "Lade $($release.tag_name) herunter ($([math]::Round($asset.size / 1MB, 1)) MB) ..."
    Invoke-WebRequest $asset.browser_download_url -OutFile $zip -Headers $headers
    Write-Step 'Entpacken ...'
    Expand-Archive -Path $zip -DestinationPath $InstallDir -Force
    Remove-Item $zip -Force
    # Von GitHub geladene Dateien als vertrauenswürdig markieren (sonst warnt Windows bei jedem Start)
    Get-ChildItem $InstallDir -Recurse | Unblock-File
    $version = $release.tag_name
}

# ------------------------------------------------------------------------------------------
Write-Step 'Verknüpfungen anlegen ...'
New-Shortcut $StartLink $Exe
if (-not $NoDesktopShortcut) { New-Shortcut $DeskLink $Exe }

Write-Host ''
Write-Host "  SlideBear $version ist installiert." -ForegroundColor Green
Write-Host "  Programm:  $InstallDir"
Write-Host "  Daten:     $DataDir"
Write-Host '  Start über das Startmenü oder die Desktop-Verknüpfung. Zum Aktualisieren das Skript einfach erneut ausführen.'
Write-Host ''

$answer = Read-Host '  SlideBear jetzt starten? (J/n)'
if ($answer -eq '' -or $answer -match '^[jJyY]') { Start-Process $Exe }
