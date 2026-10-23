[CmdletBinding()]param([switch]$StopDatabase)
$ErrorActionPreference='Stop'
$current=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'current.json') -Raw | ConvertFrom-Json
if ($current.commit -notmatch '^[a-f0-9]{40}$') {throw 'Identitas instalasi tidak valid.'}
$pidFile=Join-Path $PSScriptRoot 'server.pid'
if (Test-Path -LiteralPath $pidFile) {
 $process=Get-Process -Id ([int](Get-Content -LiteralPath $pidFile)) -ErrorAction SilentlyContinue
 if ($process) {
  $expected=Join-Path $PSScriptRoot ('versions/'+$current.commit+'/haruflow.exe')
  if ($process.Path -ne [IO.Path]::GetFullPath($expected)) {throw 'PID milik proses lain; proses tidak dihentikan.'}
  $process.Kill();if(-not $process.WaitForExit(10000)){throw 'Server belum berhenti.'}
 }
}
Write-Output 'Server instalasi ini sudah berhenti.'
if($StopDatabase -and (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'postgres.json'))) {& (Join-Path $PSScriptRoot 'Manage-Postgres.ps1') -Action Stop -InstallRoot $PSScriptRoot}
