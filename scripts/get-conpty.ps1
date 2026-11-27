# Descarga el ConPTY de Windows Terminal (conpty.dll + OpenConsole.exe) y lo
# deja junto a neon.exe. Neon lo usa solo si está ahí; si no, usa el de Windows.
#
#   powershell -ExecutionPolicy Bypass -File scripts\get-conpty.ps1
#
# Opcional: -Dest otra carpeta, -Version otra versión del paquete NuGet.
param(
    [string]$Dest = "$PSScriptRoot\..\target\release",
    [string]$Version = "1.25.261109001"
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$work = Join-Path $env:TEMP "neon-conpty-$Version"
$zip = "$work.zip"
if (-not (Test-Path $work)) {
    Write-Host "Descargando Microsoft.Windows.Console.ConPTY $Version..."
    Invoke-WebRequest "https://www.nuget.org/api/v2/package/Microsoft.Windows.Console.ConPTY/$Version" -OutFile $zip
    Expand-Archive $zip -DestinationPath $work -Force
    Remove-Item $zip
}

New-Item -ItemType Directory -Force $Dest | Out-Null
foreach ($name in "conpty.dll", "OpenConsole.exe") {
    $file = Get-ChildItem $work -Recurse -Filter $name |
        Where-Object { $_.FullName -match '(x64|win-x64)[\\/]' -and $_.FullName -notmatch 'arm' } |
        Select-Object -First 1
    if (-not $file) { throw "No encontré $name (x64) en el paquete." }
    Copy-Item $file.FullName (Join-Path $Dest $name) -Force
    Write-Host "  $name  <-  $($file.FullName.Substring($work.Length + 1))"
}
Write-Host "Listo: ConPTY moderno en $((Resolve-Path $Dest).Path)"
