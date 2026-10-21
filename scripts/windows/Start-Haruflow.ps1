[CmdletBinding()]param([switch]$NoBrowser)
$ErrorActionPreference='Stop'
$install=[IO.Path]::GetFullPath($PSScriptRoot)
$current=Get-Content -LiteralPath (Join-Path $install 'current.json') -Raw | ConvertFrom-Json
if ($current.commit -notmatch '^[a-f0-9]{40}$') { throw 'Identitas instalasi tidak valid.' }
$executable=Join-Path $install ('versions/'+$current.commit+'/haruflow.exe')
$pidFile=Join-Path $install 'server.pid'; $existing=$null
if (Test-Path -LiteralPath $pidFile) { $existing=Get-Process -Id ([int](Get-Content -LiteralPath $pidFile)) -ErrorAction SilentlyContinue }
if ($existing -and $existing.Path -ne $executable) { throw 'PID tidak menunjuk ke server instalasi ini.' }
if (-not $existing) {
    $listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,[int]$current.port)
    try {$listener.Start()} catch {throw 'Port sudah digunakan. Hentikan aplikasi lain atau pilih port berbeda saat instalasi.'} finally {$listener.Stop()}
    $logs=Join-Path $install 'logs'; New-Item -ItemType Directory -Path $logs -Force | Out-Null
    $stamp=[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff')
    $saved=@{}; $configured=@{}
    foreach ($line in [IO.File]::ReadAllLines((Join-Path $install '.env'))) {
        if ($line -match '^(DATABASE_URL|HARUFLOW_SETUP_TOKEN|HARUFLOW_BIND|HARUFLOW_STATIC_DIR|HARUFLOW_SECURE_COOKIES)=(.*)$') {
            $name=$Matches[1];$value=$Matches[2]
            if ($value.StartsWith('"') -and $value.EndsWith('"')) {$value=$value.Substring(1,$value.Length-2).Replace('\"','"').Replace('\\','\')}
            $configured[$name]=$value
        }
    }
    if ($configured.Count -ne 5) {throw 'Konfigurasi instalasi tidak lengkap.'}
    try {
        foreach ($name in $configured.Keys) {$saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process');[Environment]::SetEnvironmentVariable($name,$configured[$name],'Process')}
        $process=Start-Process -FilePath $executable -ArgumentList 'serve' -WorkingDirectory $install -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $logs ($stamp+'.stdout.log')) -RedirectStandardError (Join-Path $logs ($stamp+'.stderr.log'))
    } finally {foreach ($name in $saved.Keys) {if ([string]::IsNullOrEmpty($saved[$name])) {Remove-Item -LiteralPath ('Env:\'+$name) -ErrorAction SilentlyContinue} else {[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')}}}
    $process.Id | Set-Content -LiteralPath $pidFile
}
$url='http://127.0.0.1:'+$current.port
$ready=$false
for ($attempt=0;$attempt -lt 30;$attempt++) {
    try { $response=Invoke-WebRequest -UseBasicParsing -Uri ($url+'/health/ready') -TimeoutSec 2; if ($response.StatusCode -eq 200) {$ready=$true;break} } catch {Start-Sleep -Milliseconds 200}
}
if (-not $ready) {throw 'Server belum siap. Periksa log instalasi; database mungkin perlu dipulihkan atau dimigrasikan.'}
$version=Invoke-RestMethod -Uri ($url+'/version') -TimeoutSec 3
if ($version.commit -ne $current.commit) {throw 'Layanan pada port ini berbeda dari versi instalasi.'}
if (-not $NoBrowser) {Start-Process $url}
Write-Output "Haruflow siap di $url"
