# Makes the mainnet keys, from this folder (Documents\Tiber), with the Solana tools the devnet kit
# installed: the five program keypairs (their addresses become the programs' mainnet addresses) and
# the deployer (pays for the deploy, holds the upgrade authority, and is the admin).
#
# The keys go to C:\TiberMainnet\keys, outside Documents: Windows often syncs Documents to OneDrive,
# and these must never leave this PC except onto your own backup drives. Running it again never
# replaces a key: an existing one is kept and only its address is shown.
#
# At the end it writes C:\TiberMainnet\addresses.txt: the public addresses only, safe to share.
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$keygen = "$here\solana-tools\solana-release\bin\solana-keygen.exe"
if (-not (Test-Path $keygen)) { Write-Host "!! The Solana tools are missing: run run-devnet.bat once first."; exit 1 }
$root = "C:\TiberMainnet"
$keys = "$root\keys"
New-Item -ItemType Directory -Force -Path $keys | Out-Null

$programs = @("tiber_token", "tiber_swap", "tiber_bridge", "tiber_twins", "tiber_tithe")
$lines = @()
foreach ($p in $programs) {
  $file = "$keys\$p-keypair.json"
  if (-not (Test-Path $file)) {
    & $keygen new --no-bip39-passphrase --silent --outfile $file | Out-Null
  }
  $address = (& $keygen pubkey $file).Trim()
  $lines += "$p $address"
}

$deployer = "$keys\deployer.json"
if (-not (Test-Path $deployer)) {
  Write-Host ""
  Write-Host "== The deployer key. Below are its 12 recovery words."
  Write-Host "   Write them on paper, in order, and keep the paper somewhere safe and offline."
  Write-Host "   Never type them into a website, a chat (including Claude), or a photo."
  Write-Host ""
  & $keygen new --no-bip39-passphrase --outfile $deployer
}
$lines += "deployer " + (& $keygen pubkey $deployer).Trim()

$lines | Set-Content -Path "$root\addresses.txt"
Write-Host ""
Write-Host "== Done. The keys are in $keys"
Write-Host "   The public addresses (safe to share with Claude) are in $root\addresses.txt:"
Write-Host ""
$lines | ForEach-Object { Write-Host "   $_" }
Write-Host ""
Write-Host "   Next: copy the whole C:\TiberMainnet\keys folder onto two USB drives and keep them apart."
