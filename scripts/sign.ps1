# Firma uno o más .exe con el certificado de firma de código, si está
# configurado. Sin certificado no hace nada (el build sigue igual).
#
# En GitHub: Settings → Secrets and variables → Actions:
#   WINDOWS_CERT_PFX       el .pfx en base64  ([Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx")))
#   WINDOWS_CERT_PASSWORD  su contraseña
#
#   scripts\sign.ps1 target\release\neon.exe
param([Parameter(Mandatory, ValueFromRemainingArguments)][string[]]$Files)
$ErrorActionPreference = "Stop"

if (-not $env:WINDOWS_CERT_PFX) {
    Write-Host "Sin certificado (WINDOWS_CERT_PFX): no se firma."
    exit 0
}

$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $signtool) { throw "No encontré signtool.exe (Windows SDK)." }

$pfx = Join-Path $env:RUNNER_TEMP "neon-cert.pfx"
if (-not $env:RUNNER_TEMP) { $pfx = Join-Path $env:TEMP "neon-cert.pfx" }
[IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERT_PFX))
try {
    foreach ($f in $Files) {
        & $signtool.FullName sign /f $pfx /p $env:WINDOWS_CERT_PASSWORD `
            /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 `
            /d "Neon" $f
        if ($LASTEXITCODE -ne 0) { throw "signtool falló con $f" }
        Write-Host "Firmado: $f"
    }
} finally {
    Remove-Item $pfx -ErrorAction SilentlyContinue
}
