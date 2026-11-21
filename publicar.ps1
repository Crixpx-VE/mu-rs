# Compila el cliente y el lanzador en release y los publica en la carpeta que sirve
# mu-update-server. Los lanzadores de los jugadores los descargan al abrirse.
#
#   .\publicar.ps1 [-Destino ..\dist\cliente]
#
# Cada archivo se copia primero como .tmp y luego se renombra, para que el servidor
# nunca entregue un archivo a medio copiar.
param([string]$Destino = (Join-Path $PSScriptRoot '..\dist\cliente'))

$ErrorActionPreference = 'Stop'
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
Push-Location $PSScriptRoot
try {
    cargo build --release -p mu-client -p mu-launcher -p mu-server
    if ($LASTEXITCODE -ne 0) { throw 'la compilación falló' }

    # El servidor de juego (tarea "mu-rs servidor"): igual, si cambió se reinicia.
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
        if (Get-ScheduledTask -TaskName $tareaJuego -ErrorAction SilentlyContinue) {
            Start-ScheduledTask -TaskName $tareaJuego
            "publicado: mu-server.exe (servidor de juego reiniciado)"
        } else {
            "publicado: mu-server.exe (falta la tarea '$tareaJuego')"
        }
    }

    # El servidor de actualizaciones (tarea programada): si cambió, se para, se cambia y se arranca.
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
