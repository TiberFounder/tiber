# Upgrades the five devnet programs in place to the reviewed version (same addresses, same ROM and
# REM, nothing to set up again), from this folder (Documents\Tiber). Unzip upgrade-devnet.zip here
# first, so the new builds are in .\upgrade-programs. Safe to run again: every program already
# upgraded is skipped. Everything it prints is also saved to devnet-log.txt.
#
# What the new version changes: on the twins' pools only the owner of the tokens may sell them
# (a delegate could dodge the 5% quick-sell burn), the exchange's pause no longer stops the twins,
# and every program's security contact now points at TiberFounder/tiber.
$ErrorActionPreference = "Continue"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here
Start-Transcript -Path (Join-Path $here "devnet-log.txt") -Append | Out-Null
$url = "https://api.devnet.solana.com"
try {
  $kit = "$here\tiber-devnet-kit"
  $bin = "$here\solana-tools\solana-release\bin"
  $solana = "$bin\solana.exe"
  $keygen = "$bin\solana-keygen.exe"
  $new = "$here\upgrade-programs"
  if (-not (Test-Path $solana) -or -not (Test-Path $kit)) { throw "run run-devnet.bat first (the devnet kit is missing)" }
  if (-not (Test-Path $new)) { throw "unzip upgrade-devnet.zip into this folder first (upgrade-programs is missing)" }
  $env:PATH = "$bin;$env:PATH"
  if (-not (Get-Command node -ErrorAction SilentlyContinue)) { $env:PATH = "$here\node-v22.12.0-win-x64;$env:PATH" }
  $deployer = "$kit\keys\deployer.json"
  $address = (& $keygen pubkey $deployer).Trim()

  # The devnet addresses the new builds are made for. A kit key that doesn't match stops everything.
  $ids = [ordered]@{
    tiber_token  = "CFW2Z5J3dXwRXJzpxxfZiTc27KXuggDyzfZU7L6B6a8e"
    tiber_bridge = "4WXBYj7efT8SYhRxacekrp2zgmqoPnyGQifE6obRGLeG"
    tiber_swap   = "7nUWnHYtqgLv2sevNxxVBVqZSKd5VZqLBTWbf7un3kSM"
    tiber_twins  = "e1UkVXrsWoQW97Yw8eCP1NJ3kcdqLYNgwQwidL7Lg89"
    tiber_tithe  = "5WNWnM3JTDgDYkA4Mh7quKQXtkdiwF3d7bgc9RuX4KF"
  }
  foreach ($p in $ids.Keys) {
    if (-not (Test-Path "$new\$p.so")) { throw "upgrade-programs\$p.so is missing; unzip upgrade-devnet.zip again" }
    $id = (& $keygen pubkey "$kit\keys\$p-keypair.json").Trim()
    if ($id -ne $ids[$p]) { throw "$p's key in the kit is $id, not $($ids[$p]); stop and tell Claude" }
  }

  $marks = "$kit\upgrade-review"
  New-Item -ItemType Directory -Force -Path $marks | Out-Null
  $sol = { [double](((& $solana balance -u $url -k $deployer) -split " ")[0]) }
  $rentOf = { param($bytes) [double]((((& $solana rent -u $url -k $deployer $bytes) -join " ") -replace '.*minimum:\s*([0-9.]+).*', '$1')) }

  # What the upgrades need: the largest program's rent for the few minutes its upgrade buffer
  # exists (refunded right after), plus a little per program for fees and any growth.
  $todo = @($ids.Keys | Where-Object { -not (Test-Path "$marks\$_.done") })
  $largest = 0.0
  foreach ($p in $todo) {
    $r = & $rentOf ((Get-Item "$new\$p.so").Length + 45)
    if ($r -gt $largest) { $largest = $r }
  }
  $need = [math]::Round($largest + 0.05 * $todo.Count + 0.1, 2)
  $balance = & $sol
  Write-Host "deployer $address has $balance devnet SOL; the upgrades need about $need"
  if ($todo.Count -gt 0 -and $balance -lt $need) {
    Write-Host "!! Not enough devnet SOL yet. Get more free at https://faucet.solana.com for:"
    Write-Host "   $address"
    Write-Host "   then run this again."
    exit 2
  }

  foreach ($p in $todo) {
    $so = "$new\$p.so"
    Write-Host "== Upgrading $p ($($ids[$p]), $((Get-Item $so).Length) bytes)"
    & $solana program deploy -u $url -k $deployer --program-id "$kit\keys\$p-keypair.json" --upgrade-authority $deployer --max-sign-attempts 30 $so
    if ($LASTEXITCODE -ne 0) { throw "upgrade of $p failed; run this again to resume" }
    Copy-Item -Force $so "$kit\programs\$p.so"
    New-Item -ItemType File -Force -Path "$marks\$p.done" | Out-Null
  }

  $left = & $sol
  if ($left -lt 2.2) {
    Write-Host "!! All five programs are upgraded. The checks need about 2.2 devnet SOL for test wallets; the deployer has $left."
    Write-Host "   Get more at https://faucet.solana.com for $address, then run this again (it goes straight to the checks)."
    exit 3
  }
  Set-Location "$kit\client"
  Write-Host "== Checking every rule on the upgraded programs"
  npx tsx scripts/scenario.ts $url ..\keys\deployer.json 2>&1 | ForEach-Object { Write-Host $_ }
  Write-Host "== Finished. Tell Claude how many checks passed (the last lines above)."
} catch {
  Write-Host "!! $_"
} finally {
  Stop-Transcript | Out-Null
}
