# Builds the client and launcher in release and publishes them to the folder served by
# mu-update-server. Players' launchers download them on startup.
#
#   .\publicar.ps1 [-Destino ..\dist\cliente]
#
# Each file is first copied as .tmp and then renamed, so the server
# never serves a half-copied file.
param([string]$Destino = (Join-Path $PSScriptRoot '..\dist\cliente'))

$ErrorActionPreference = 'Stop'
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
Push-Location $PSScriptRoot
try {
    cargo build --release -p mu-client -p mu-launcher -p mu-server
    if ($LASTEXITCODE -ne 0) { throw 'la compilación falló' }

    # The game server (task "mu-rs servidor"): likewise, restarted if it changed.
    $tareaJuego = 'mu-rs servidor'
    $juego = Join-Path (Split-Path $Destino) 'servidor\mu-server.exe'
    $nuevoJuego = 'target\release\mu-server.exe'
    if (-not (Test-Path $juego) -or ((Get-FileHash $nuevoJuego).Hash -ne (Get-FileHash $juego).Hash)) {
        if (Get-ScheduledTask -TaskName $tareaJuego -ErrorAction SilentlyContinue) {
            Stop-ScheduledTask -TaskName $tareaJuego
            Get-Process mu-server -ErrorAction SilentlyContinue | Stop-Process -Force
            Start-Sleep -Seconds 1
        }
        Copy-Item $nuevoJuego $juego -Force
        $t = Get-ScheduledTask -TaskName $tareaJuego -ErrorAction SilentlyContinue
        if ($t -and $t.State -eq 'Disabled') {
            # Disabled while the client is tested against OpenMU (task "mu-openmu", same ports).
            "publicado: mu-server.exe (tarea '$tareaJuego' desactivada: no se arranca)"
        } elseif ($t) {
            Start-ScheduledTask -TaskName $tareaJuego
            "publicado: mu-server.exe (servidor de juego reiniciado)"
        } else {
            "publicado: mu-server.exe (falta la tarea '$tareaJuego')"
        }
    }

    # The update server (scheduled task): if it changed, stop it, replace it and start it.
    $tarea = 'mu-rs actualizaciones'
    $servidor = Join-Path (Split-Path $Destino) 'servidor\mu-update-server.exe'
    $nuevo = 'target\release\mu-update-server.exe'
    if ((Test-Path $servidor) -and ((Get-FileHash $nuevo).Hash -ne (Get-FileHash $servidor).Hash)) {
        Stop-ScheduledTask -TaskName $tarea
        Get-Process mu-update-server -ErrorAction SilentlyContinue | Stop-Process -Force
        Start-Sleep -Seconds 1
        Copy-Item $nuevo $servidor -Force
        Start-ScheduledTask -TaskName $tarea
        "publicado: mu-update-server.exe (servidor reiniciado)"
    }

    New-Item -ItemType Directory -Force $Destino | Out-Null
    foreach ($exe in 'mu-client.exe', 'mu-launcher.exe') {
        $origen = Join-Path 'target\release' $exe
        $final = Join-Path $Destino $exe
        if ((Test-Path $final) -and ((Get-FileHash $origen).Hash -eq (Get-FileHash $final).Hash)) {
            "sin cambios: $exe"
            continue
        }
        Copy-Item $origen "$final.tmp" -Force
        Move-Item "$final.tmp" $final -Force
        "publicado: $exe"
    }
} finally {
    Pop-Location
}
