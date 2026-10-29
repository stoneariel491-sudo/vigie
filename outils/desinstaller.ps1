# Desinstalle Vigie : ce que l'installation a pose, et rien d'autre.
#
# Lance depuis « Applications installees », ou a la main. Les donnees de la
# vigie (%APPDATA%\Vigie : sa fenetre, son catalogue) restent, sauf avec
# -AussiLesDonnees : les effacer, c'est perdre une minute au prochain
# lancement, et l'on ne le fait pas sans qu'on l'ait demande.

param([switch]$AussiLesDonnees)

$cible = Join-Path $env:LOCALAPPDATA 'Programs\Vigie'
$exe = Join-Path $cible 'Vigie.exe'

Get-Process -Name vigie -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe } | ForEach-Object {
    $_.CloseMainWindow() | Out-Null
    if (-not $_.WaitForExit(5000)) { $_.Kill() }
}

foreach ($lien in @(
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'Vigie.lnk'),
    (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Vigie.lnk')
)) {
    # Seulement un raccourci qui vise CETTE installation : un autre « Vigie »
    # sur le Bureau n'est pas a nous.
    if (Test-Path $lien) {
        $vise = (New-Object -ComObject WScript.Shell).CreateShortcut($lien).TargetPath
        if ($vise -eq $exe) { Remove-Item $lien }
    }
}

Remove-Item 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Vigie' -Recurse -ErrorAction SilentlyContinue
# Le script se trouve dans le dossier qu'il efface : Windows le laisse faire,
# puisque PowerShell l'a deja lu en entier.
Remove-Item $cible -Recurse -Force -ErrorAction SilentlyContinue
if ($AussiLesDonnees) { Remove-Item (Join-Path $env:APPDATA 'Vigie') -Recurse -Force -ErrorAction SilentlyContinue }
Write-Output 'Vigie desinstallee.'
