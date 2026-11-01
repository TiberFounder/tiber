# Romulus and Remus on devnet, from this folder (Documents\Tiber). Safe to run again: every step
# that is already done is skipped. Everything it prints is also saved to devnet-log.txt.
$ErrorActionPreference = "Continue"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here
Start-Transcript -Path (Join-Path $here "devnet-log.txt") -Append | Out-Null
$url = "https://api.devnet.solana.com"
try {
  # 1. The kit.
  if (-not (Test-Path "$here\tiber-devnet-kit")) {
    Write-Host "== Unzipping the kit"
    Expand-Archive -Path "$here\tiber-devnet-kit.zip" -DestinationPath $here
  }
  $kit = "$here\tiber-devnet-kit"

  # 2. The Solana tools (v4.3.0, from Anza's release server), kept in this folder only.
  $tools = "$here\solana-tools"
  $solana = "$tools\solana-release\bin\solana.exe"
  if (-not (Test-Path $solana)) {
    Write-Host "== Downloading the Solana tools v4.3.0 from release.anza.xyz"
    New-Item -ItemType Directory -Force -Path $tools | Out-Null
    $tar = "$tools\solana-release.tar.bz2"
    Invoke-WebRequest -Uri "https://release.anza.xyz/v4.3.0/solana-release-x86_64-pc-windows-msvc.tar.bz2" -OutFile $tar
    tar -xjf $tar -C $tools
    Remove-Item $tar
  }
  $env:PATH = "$tools\solana-release\bin;$env:PATH"
  & $solana --version

  # 3. Node.js: a portable copy kept in this folder (nothing is installed on the PC).
  if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    $nodeDir = "$here\node-v22.12.0-win-x64"
    if (-not (Test-Path "$nodeDir\node.exe")) {
      Write-Host "== Downloading a portable Node.js v22.12.0 from nodejs.org"
      $zip = "$here\node.zip"
      $ProgressPreference = "SilentlyContinue"
      Invoke-WebRequest -Uri "https://nodejs.org/dist/v22.12.0/node-v22.12.0-win-x64.zip" -OutFile $zip
      Expand-Archive -Path $zip -DestinationPath $here
      Remove-Item $zip
    }
    $env:PATH = "$nodeDir;$env:PATH"
  }
  Write-Host "node $(node --version)"

  # 4. Devnet SOL on the deployer.
  $deployer = "$kit\keys\deployer.json"
  $address = (& "$tools\solana-release\bin\solana-keygen.exe" pubkey $deployer).Trim()
  $balance = [double](((& $solana balance -u $url -k $deployer) -split " ")[0])
  Write-Host "deployer $address has $balance devnet SOL"
  # What the deploy still needs: the rent of every program not yet deployed, plus the largest one
  # again for the few minutes its deploy buffer exists (refunded right after).
  $need = 0.2
  $largest = 0.0
  foreach ($p in "tiber_token","tiber_swap","tiber_bridge","tiber_twins","tiber_tithe") {
    $id = (& "$tools\solana-release\bin\solana-keygen.exe" pubkey "$kit\keys\$p-keypair.json").Trim()
    & $solana program show -u $url -k $deployer $id 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) {
      $size = (Get-Item "$kit\programs\$p.so").Length + 45
      $rent = [double]((((& $solana rent -u $url -k $deployer $size) -join " ") -replace '.*minimum:\s*([0-9.]+).*', '$1'))
      $need += $rent
      if ($rent -gt $largest) { $largest = $rent }
    }
  }
  $need = [math]::Round($need + $largest, 2)
  Write-Host "the deploy needs about $need devnet SOL"
  if ($balance -lt $need) {
    Write-Host "!! Not enough devnet SOL yet. Get more free at https://faucet.solana.com for:"
    Write-Host "   $address"
    Write-Host "   then run this again."
    exit 2
  }

  # 5. Deploy the five programs, one at a time.
  foreach ($p in "tiber_token","tiber_swap","tiber_bridge","tiber_twins","tiber_tithe") {
    $key = "$kit\keys\$p-keypair.json"
    $so = "$kit\programs\$p.so"
    $id = (& "$tools\solana-release\bin\solana-keygen.exe" pubkey $key).Trim()
    & $solana program show -u $url -k $deployer $id 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { Write-Host "skip  $p is deployed ($id)"; continue }
    $size = (Get-Item $so).Length
    Write-Host "== Deploying $p ($id, $size bytes)"
    & $solana program deploy -u $url -k $deployer --program-id $key --upgrade-authority $deployer --max-len $size --max-sign-attempts 30 $so
    if ($LASTEXITCODE -ne 0) { throw "deploy of $p failed; run this again to resume" }
  }

  # 6. Set the twins up (trading stays closed), then open trading and check every rule straight
  #    away, once there is enough SOL left for the test wallets.
  Set-Location "$kit\client"
  if (-not (Test-Path "node_modules")) { Write-Host "== Installing the client"; npm install --no-audit --no-fund 2>&1 | ForEach-Object { Write-Host $_ } }
  Write-Host "== Setting up Romulus and Remus"
  npx tsx scripts/setup.ts $url ..\keys\deployer.json 2>&1 | ForEach-Object { Write-Host $_ }
  if ($LASTEXITCODE -ne 0) { throw "setup failed; run this again to resume" }
  $left = [double](((& $solana balance -u $url -k $deployer) -split " ")[0])
  if ($left -lt 2.2) {
    Write-Host "!! Deployed and set up. The checks need about 2.2 devnet SOL for test wallets; the deployer has $left."
    Write-Host "   Get more at https://faucet.solana.com for $address, then run this again."
    exit 3
  }
  Write-Host "== Opening trading"
  npx tsx scripts/setup.ts $url ..\keys\deployer.json --open 2>&1 | ForEach-Object { Write-Host $_ }
  Write-Host "== Checking every rule"
  npx tsx scripts/scenario.ts $url ..\keys\deployer.json 2>&1 | ForEach-Object { Write-Host $_ }
  Write-Host "== Finished. ROM and REM addresses are in tiber-devnet-kit\client\deployments\devnet.json"
} catch {
  Write-Host "!! $_"
} finally {
  Stop-Transcript | Out-Null
}
