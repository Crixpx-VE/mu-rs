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
    cargo build --release -p mu-client -p mu-launcher
    if ($LASTEXITCODE -ne 0) { throw 'la compilación falló' }

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
