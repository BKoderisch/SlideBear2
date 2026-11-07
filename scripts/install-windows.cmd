@echo off
rem SlideBear2 fuer Windows installieren oder aktualisieren: Datei doppelklicken.
rem Funktioniert auch, wenn PowerShell-Skripte (.ps1) gesperrt sind, weil das Installationsskript
rem als Text an PowerShell uebergeben wird statt als Skriptdatei ausgefuehrt zu werden.
rem Optionen werden durchgereicht, z. B. in der Eingabeaufforderung:
rem   install-windows.cmd -Uninstall
rem   install-windows.cmd -NoDesktopShortcut
setlocal
set "URL=https://raw.githubusercontent.com/BKoderisch/SlideBear2/main/scripts/install-windows.ps1"
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "& ([scriptblock]::Create((Invoke-RestMethod '%URL%').TrimStart([char]0xFEFF))) %*"
if errorlevel 1 (
    echo.
    echo   Die Installation ist fehlgeschlagen, siehe Meldung oben.
    pause
)
endlocal
