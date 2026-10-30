# Installe Vigie pour l'utilisateur courant, sans droits d'administrateur :
#
#   .\outils\installer.ps1
#
# Ce qu'il fait, et rien d'autre :
#   1. compile en release (sauf -SansCompiler) ;
#   2. ferme la vigie ouverte, s'il y en a une, et copie l'executable dans
#      %LOCALAPPDATA%\Programs\Vigie, l'endroit des applications installees par
#      utilisateur sous Windows ;
#   3. copie l'icone a cote de l'executable (Vigie.ico) et pose un raccourci
#      dans le menu Demarrer, et refait celui du Bureau ;
#   4. l'inscrit dans « Applications installees », avec son icone, sa version et
#      sa desinstallation.
#
# Relancer ce script met a jour l'installation. Les donnees de la vigie
# (%APPDATA%\Vigie : sa fenetre, son catalogue) ne sont jamais touchees.

param([switch]$SansCompiler)
$ErrorActionPreference = 'Stop'

$depot = Split-Path -Parent $PSScriptRoot
$cible = Join-Path $env:LOCALAPPDATA 'Programs\Vigie'
$exe = Join-Path $cible 'Vigie.exe'
$menu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Vigie.lnk'
$bureau = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Vigie.lnk'
$cle = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Vigie'

if (-not $SansCompiler) {
    Push-Location $depot
    try { cargo build --release; if ($LASTEXITCODE -ne 0) { throw 'la compilation a echoue' } }
    finally { Pop-Location }
}
$source = Join-Path $depot 'target\release\vigie.exe'
if (-not (Test-Path $source)) { throw "introuvable : $source" }

# Une vigie ouverte tient son executable : on la ferme d'abord, et l'on attend
# qu'elle soit partie. Elle garde son etat en se fermant.
$ouvertes = Get-Process -Name vigie -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }
foreach ($p in $ouvertes) { $p.CloseMainWindow() | Out-Null }
foreach ($p in $ouvertes) { if (-not $p.WaitForExit(5000)) { $p.Kill() } }

New-Item -ItemType Directory -Force $cible | Out-Null
# Un executable qu'un autre programme garde ouvert ne s'ecrase pas, mais se
# renomme : l'ancien part de cote et sera efface a la prochaine installation.
Get-ChildItem $cible -Filter 'Vigie.*.ancien' -ErrorAction SilentlyContinue | Remove-Item -ErrorAction SilentlyContinue
if (Test-Path $exe) {
    try { Remove-Item $exe }
    catch { Rename-Item $exe ("Vigie.{0}.ancien" -f (Get-Date -Format 'yyyyMMddHHmmss')) }
}
Copy-Item $source $exe
Copy-Item (Join-Path $PSScriptRoot 'desinstaller.ps1') (Join-Path $cible 'desinstaller.ps1') -Force

# L'icone, dans un fichier a elle. Le 2026-10-29, apres le changement d'icone,
# le menu Demarrer montrait la nouvelle et le bouton de la barre des taches
# l'ancienne : Windows garde, pour un raccourci, l'icone qu'il a vue sous le
# couple (fichier, numero) donne, et « Vigie.exe,0 » etait deja dans son cache.
# Un fichier au nom jamais vu n'a pas de cache : le raccourci, et avec lui le
# bouton, prennent l'icone du fichier. Le numero d'une icone qui devrait un
# jour changer encore se change avec le nom du fichier (Vigie.ico -> Vigie2.ico).
$icone = Join-Path $cible 'Vigie.ico'
Copy-Item (Join-Path $depot 'assets/vigie.ico') $icone -Force

$coquille = New-Object -ComObject WScript.Shell
foreach ($lien in @($menu, $bureau)) {
    $r = $coquille.CreateShortcut($lien)
    $r.TargetPath = $exe
    $r.WorkingDirectory = $cible
    $r.IconLocation = "$icone,0"
    $r.Description = 'Regarder penser les agents, en direct'
    $r.Save()
}

$version = (Select-String -Path (Join-Path $depot 'Cargo.toml') -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$taille = [int]((Get-Item $exe).Length / 1KB)
New-Item -Path $cle -Force | Out-Null
$desinstaller = "powershell.exe -NoProfile -File `"$cible\desinstaller.ps1`""
@{
    DisplayName     = 'Vigie'
    DisplayVersion  = $version
    Publisher       = 'Stone'
    DisplayIcon     = "$icone,0"
    InstallLocation = $cible
    UninstallString = $desinstaller
    QuietUninstallString = $desinstaller
}.GetEnumerator() | ForEach-Object { New-ItemProperty -Path $cle -Name $_.Key -Value $_.Value -PropertyType String -Force | Out-Null }
New-ItemProperty -Path $cle -Name EstimatedSize -Value $taille -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path $cle -Name NoModify -Value 1 -PropertyType DWord -Force | Out-Null
New-ItemProperty -Path $cle -Name NoRepair -Value 1 -PropertyType DWord -Force | Out-Null

Write-Output "Vigie $version installee dans $cible"
Write-Output "  menu Demarrer : $menu"
Write-Output "  Bureau        : $bureau"
