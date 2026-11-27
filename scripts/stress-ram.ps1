# Prueba de estrés de RAM para Neon. Córrelo DENTRO de una pestaña de Neon:
#
#   powershell -ExecutionPolicy Bypass -File scripts\stress-ram.ps1
#
# Llena el historial de esta pestaña (30.000 líneas anchas con colores, más
# que el límite de 10.000) y muestra cuánta memoria usa Neon antes y después.
# Para el "máximo esfuerzo", ábrelo en varias pestañas/paneles y mira el total.

function Neon-Ram($label) {
    $p = Get-Process neon -ErrorAction SilentlyContinue | Sort-Object WorkingSet64 -Descending | Select-Object -First 1
    if (-not $p) { Write-Host "No encuentro el proceso neon"; return }
    "{0,-34} en uso: {1,6:N1} MB   privada: {2,6:N1} MB   pico: {3,6:N1} MB" -f `
        $label, ($p.WorkingSet64 / 1MB), ($p.PrivateMemorySize64 / 1MB), ($p.PeakWorkingSet64 / 1MB)
}

Neon-Ram "Antes"
$e = [char]27
$texto = ("lorem ipsum dolor sit amet consectetur " * 4).Substring(0, 130)
$sb = [System.Text.StringBuilder]::new()
for ($i = 0; $i -lt 30000; $i++) {
    [void]$sb.Append("$e[3$($i % 7 + 1)m{0:D6}$e[0m $texto`n" -f $i)
    if ($i % 2000 -eq 1999) { [Console]::Write($sb.ToString()); [void]$sb.Clear() }
}
[Console]::Write($sb.ToString())
Start-Sleep -Seconds 2
Neon-Ram "Después de llenar el historial"
