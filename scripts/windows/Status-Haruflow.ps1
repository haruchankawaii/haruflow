[CmdletBinding()]
param([string]$InstallRoot=(Join-Path $env:LOCALAPPDATA 'HaruflowCommunity'),[switch]$CheckDatabase,[switch]$OpenRecoveryGuide)
$ErrorActionPreference='Stop';. (Join-Path $PSScriptRoot 'Windows.Common.ps1')
$root=Assert-HaruflowPath $InstallRoot;$current=Get-Content -LiteralPath (Join-Path $root 'current.json') -Raw | ConvertFrom-Json
if($current.commit -notmatch '^[a-f0-9]{40}$'){throw 'Identitas instalasi tidak valid.'}
Write-Output ('Haruflow '+$current.commit.Substring(0,12)+'; schema '+$current.schema_version+'; URL http://127.0.0.1:'+$current.port)
try{$response=Invoke-RestMethod ('http://127.0.0.1:'+$current.port+'/version') -TimeoutSec 3;if($response.commit -ne $current.commit){throw 'Versi berbeda'};Write-Output 'Layanan: berjalan dengan versi yang sesuai.'}catch{Write-Output 'Layanan: berhenti/tidak sesuai. Jalankan Start-Haruflow.ps1 atau periksa logs/.'}
& (Join-Path $root 'Manage-Postgres.ps1') -Action Status -InstallRoot $root
& (Join-Path $root 'Schedule-Haruflow.ps1') -Action Status -InstallRoot $root
$stateFile=Join-Path $root 'backup-state.json';if(Test-Path -LiteralPath $stateFile){$state=Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json;Write-Output ('Backup terakhir: '+$state.status+'; '+$state.completed_at);if($state.status -eq 'success'){Write-Output ('Arsip: '+$state.archive)}}else{Write-Output 'Backup terkelola: belum pernah berhasil. Jalankan Run-Backup.ps1 setelah menentukan pengaturan.'}
if($CheckDatabase){
 $saved=[Environment]::GetEnvironmentVariable('DATABASE_URL','Process')
 try{
  $configured=$false
  foreach($line in [IO.File]::ReadAllLines((Join-Path $root '.env'))){if($line -match '^DATABASE_URL=(.*)$'){if($configured){throw 'DATABASE_URL ganda dalam konfigurasi.'};$value=$Matches[1];if($value.StartsWith('"') -and $value.EndsWith('"')){$value=$value.Substring(1,$value.Length-2).Replace('\"','"').Replace('\\','\')};$env:DATABASE_URL=$value;$configured=$true}}
  if(-not $configured -or [string]::IsNullOrWhiteSpace($env:DATABASE_URL)){throw 'Koneksi database instalasi tidak lengkap.'}
  Push-Location -LiteralPath $root;try{& (Join-Path $root ('versions/'+$current.commit+'/haruflow.exe')) database check;if($LASTEXITCODE -ne 0){throw 'Pemeriksaan database gagal; periksa hasil diagnosis sebelum melanjutkan.'}}finally{Pop-Location}
 }finally{if($null -eq $saved){Remove-Item Env:DATABASE_URL -ErrorAction SilentlyContinue}else{[Environment]::SetEnvironmentVariable('DATABASE_URL',$saved,'Process')}}
}
$guide=Join-Path $root ('versions/'+$current.commit+'/source/docs/WINDOWS_BACKUP.md');Write-Output ('Panduan pemulihan: '+$guide)
if($OpenRecoveryGuide){Start-Process -FilePath notepad.exe -ArgumentList ('"'+$guide+'"')}
