# Brings devnet up to the loyalty-burn version, from this folder (Documents\Tiber). Run it after
# run-devnet.ps1 has deployed once. Safe to run again: every step that is done is skipped.
#   1. Upgrades the bridge (the Wall), the twins and the tithe in place (same program addresses).
#   2. Puts the first ROM/REM pair aside (its mints can't take the Wall's arrival note) and makes a
#      new pair, then opens trading and runs every check, as run-devnet.ps1 does.
# Everything it prints is also saved to devnet-log.txt.
$ErrorActionPreference = "Continue"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here
Start-Transcript -Path (Join-Path $here "devnet-log.txt") -Append | Out-Null
$url = "https://api.devnet.solana.com"
try {
  $kit = "$here\tiber-devnet-kit"
  $bin = "$here\solana-tools\solana-release\bin"
  $solana = "$bin\solana.exe"
  if (-not (Test-Path $solana) -or -not (Test-Path $kit)) { throw "run run-devnet.bat first" }
  $env:PATH = "$bin;$env:PATH"
  if (-not (Get-Command node -ErrorAction SilentlyContinue)) { $env:PATH = "$here\node-v22.12.0-win-x64;$env:PATH" }
  $deployer = "$kit\keys\deployer.json"
  $address = (& "$bin\solana-keygen.exe" pubkey $deployer).Trim()
  $marks = "$kit\refresh"
  New-Item -ItemType Directory -Force -Path $marks | Out-Null
  $sol = { [double](((& $solana balance -u $url -k $deployer) -split " ")[0]) }
  $rentOf = { param($bytes) [double]((((& $solana rent -u $url -k $deployer $bytes) -join " ") -replace '.*minimum:\s*([0-9.]+).*', '$1')) }

  # What the upgrades need: a little for each program's growth, plus the largest one for the few minutes its
  # upgrade buffer exists (refunded right after), plus about 0.5 for the new pair.
  $todo = @("tiber_bridge","tiber_twins","tiber_tithe") | Where-Object { -not (Test-Path "$marks\$_.done") }
  $need = 0.5
  $largest = 0.0
  foreach ($p in $todo) {
    $size = (Get-Item "$kit\programs\$p.so").Length
    $r = & $rentOf ($size + 45)
    if ($r -gt $largest) { $largest = $r }
    $need += 0.05
  }
  $need = [math]::Round($need + $largest, 2)
  $balance = & $sol
  Write-Host "deployer $address has $balance devnet SOL; the upgrade and the new pair need about $need"
  if ($balance -lt $need) {
    Write-Host "!! Not enough devnet SOL yet. Get more free at https://faucet.solana.com for:"
    Write-Host "   $address"
    Write-Host "   then run this again."
    exit 2
  }

  # 1. Upgrade in place (the Solana tool grows a program's space itself when the new build is bigger).
  foreach ($p in $todo) {
    $id = (& "$bin\solana-keygen.exe" pubkey "$kit\keys\$p-keypair.json").Trim()
    $so = "$kit\programs\$p.so"
    $size = (Get-Item $so).Length
    Write-Host "== Upgrading $p ($id, $size bytes)"
    & $solana program deploy -u $url -k $deployer --program-id $id --upgrade-authority $deployer --max-sign-attempts 30 $so
    if ($LASTEXITCODE -ne 0) { throw "upgrade of $p failed; run this again to resume" }
    New-Item -ItemType File -Force -Path "$marks\$p.done" | Out-Null
  }

  # 2. A new pair: put the first one's record aside so setup makes fresh mints.
  $record = "$kit\client\deployments\devnet.json"
  if (-not (Test-Path "$marks\new-pair.done")) {
    if (Test-Path $record) { Move-Item -Force $record "$kit\client\deployments\devnet-first-pair.json" }
    New-Item -ItemType File -Force -Path "$marks\new-pair.done" | Out-Null
  }
  Set-Location "$kit\client"
  Write-Host "== Setting up the new Romulus and Remus"
  npx tsx scripts/setup.ts $url ..\keys\deployer.json 2>&1 | ForEach-Object { Write-Host $_ }
  if ($LASTEXITCODE -ne 0) { throw "setup failed; run this again to resume" }
  $left = & $sol
  if ($left -lt 2.2) {
    Write-Host "!! Upgraded and set up. The checks need about 2.2 devnet SOL for test wallets; the deployer has $left."
    Write-Host "   Get more at https://faucet.solana.com for $address, then run this again."
    exit 3
  }
  Write-Host "== Opening trading"
  npx tsx scripts/setup.ts $url ..\keys\deployer.json --open 2>&1 | ForEach-Object { Write-Host $_ }
  Write-Host "== Checking every rule"
  npx tsx scripts/scenario.ts $url ..\keys\deployer.json 2>&1 | ForEach-Object { Write-Host $_ }
  Write-Host "== Finished. Tell Claude it's done so the website can switch to the new pair."
} catch {
  Write-Host "!! $_"
} finally {
  Stop-Transcript | Out-Null
}
