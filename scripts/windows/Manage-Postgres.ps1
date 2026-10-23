[CmdletBinding()]
param([ValidateSet('Initialize','Start','Stop','Status')][string]$Action='Status',[string]$InstallRoot=(Join-Path $env:LOCALAPPDATA 'HaruflowCommunity'),[string]$PostgresBin,[ValidateRange(1024,65535)][int]$Port=55433)
$ErrorActionPreference='Stop';. (Join-Path $PSScriptRoot 'Windows.Common.ps1')
$root=Assert-HaruflowPath $InstallRoot;$config=Get-HaruflowManagedPostgres $root
if($Action -eq 'Initialize'){
    if(Test-Path -LiteralPath (Join-Path $root '.env')){throw 'Instalasi sudah memiliki koneksi database. Jangan membuat cluster baru di atas instalasi ini.'}
    $parent=Join-Path $root 'postgres';$data=Join-Path $parent 'data';$secretPath=Join-Path $parent 'init-password.tmp'
    $adminConnection=Join-Path $parent 'admin-connection.txt';$connection=Join-Path $parent 'connection.txt'
    if($config){
        # Only a deliberately recorded, unfinished bootstrap may resume. A
        # completed or unrecognised cluster is never reinitialised.
        if(-not $config.bootstrap_pending){throw 'Cluster privat sudah tercatat; gunakan Start. Data lama tidak diinisialisasi ulang.'}
        $bin=$config.bin;$Port=[int]$config.port
        foreach($file in @($adminConnection,$connection)){$null=Assert-HaruflowPath $file;if(-not(Test-Path -LiteralPath $file -PathType Leaf)){throw 'Kredensial bootstrap tidak lengkap; data dipertahankan. Periksa panduan pemulihan.'}}
        $adminUri=[Uri][IO.File]::ReadAllText($adminConnection);$appUri=[Uri][IO.File]::ReadAllText($connection)
        if($adminUri.Scheme -ne 'postgres' -or $adminUri.Host -ne '127.0.0.1' -or $adminUri.Port -ne $Port -or $adminUri.AbsolutePath -ne '/postgres' -or $adminUri.UserInfo -notmatch '^haruflow_cluster_admin:([a-f0-9]{64})$' -or $adminUri.Query -or $adminUri.Fragment){throw 'Identitas koneksi bootstrap admin tidak valid.'};$password=$Matches[1]
        if($appUri.Scheme -ne 'postgres' -or $appUri.Host -ne '127.0.0.1' -or $appUri.Port -ne $Port -or $appUri.AbsolutePath -ne '/haruflow' -or $appUri.UserInfo -notmatch '^haruflow:([a-f0-9]{64})$' -or $appUri.Query -or $appUri.Fragment){throw 'Identitas koneksi bootstrap aplikasi tidak valid.'};$appPassword=$Matches[1]
        $adminUri=$null;$appUri=$null
    }else{
        $bin=Find-HaruflowPostgres $PostgresBin -Server
        if(Test-Path -LiteralPath $parent){throw 'Folder postgres sudah ada; periksa data lama. Tidak ada file yang dihapus.'}
        $listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,$Port);try{$listener.Start()}catch{throw 'Port PostgreSQL privat sudah digunakan; pilih -Port lain.'}finally{$listener.Stop()}
        [IO.Directory]::CreateDirectory($parent) | Out-Null;Protect-HaruflowPath $parent -Directory
        [IO.File]::WriteAllText($secretPath,'');Protect-HaruflowPath $secretPath
        $random=New-Object byte[] 32;$rng=[Security.Cryptography.RandomNumberGenerator]::Create();try{$rng.GetBytes($random);$password=([BitConverter]::ToString($random).Replace('-','').ToLowerInvariant());$rng.GetBytes($random);$appPassword=([BitConverter]::ToString($random).Replace('-','').ToLowerInvariant())}finally{$rng.Dispose()}
    }
    $saved=Save-HaruflowPgEnvironment
    try{
        if(-not $config){
            [IO.File]::WriteAllText($secretPath,$password,[Text.UTF8Encoding]::new($false))
            $null=& (Join-Path $bin 'initdb.exe') -D $data --username=haruflow_cluster_admin --auth=scram-sha-256 --encoding=UTF8 --locale=C ('--pwfile='+$secretPath) 2>&1
            if($LASTEXITCODE -ne 0){throw 'initdb gagal; folder privat dipertahankan untuk diagnosis.'}
            [IO.File]::AppendAllText((Join-Path $data 'postgresql.conf'),"`nlisten_addresses = '127.0.0.1'`nport = $Port`npassword_encryption = 'scram-sha-256'`n",[Text.UTF8Encoding]::new($false))
            # Persist both private credentials before launching pg_ctl. A
            # failed start must not discard the cluster's only admin password.
            foreach($file in @($adminConnection,$connection)){[IO.File]::WriteAllText($file,'');Protect-HaruflowPath $file}
            [IO.File]::WriteAllText($adminConnection,('postgres://haruflow_cluster_admin:'+ $password+'@127.0.0.1:'+$Port+'/postgres'),[Text.UTF8Encoding]::new($false))
            [IO.File]::WriteAllText($connection,('postgres://haruflow:'+ $appPassword+'@127.0.0.1:'+$Port+'/haruflow'),[Text.UTF8Encoding]::new($false))
            $config=@{format='haruflow-private-postgres-v1';owner_sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value;data_directory=$data;bin=$bin;port=$Port;bootstrap_pending=$true;created_at=[DateTime]::UtcNow.ToString('o')}
            Write-HaruflowPrivateJson (Join-Path $root 'postgres.json') $config
        }
        & $PSCommandPath -Action Start -InstallRoot $root
        $env:PGHOST='127.0.0.1';$env:PGPORT=[string]$Port;$env:PGUSER='haruflow_cluster_admin';$env:PGPASSWORD=$password;$env:PGDATABASE='postgres';$env:PGCONNECT_TIMEOUT='5'
        $role=& (Join-Path $bin 'psql.exe') -X -w -Atq -v ON_ERROR_STOP=1 -c "SELECT rolsuper::text||':'||rolcreatedb::text||':'||rolcreaterole::text||':'||rolreplication::text||':'||rolbypassrls::text FROM pg_roles WHERE rolname='haruflow';" 2>&1
        if($LASTEXITCODE -ne 0 -or ($role -and $role -ne 'false:false:false:false:false')){throw 'Role bootstrap tidak sesuai; database tidak diubah.'}
        # Password goes through stdin, never psql arguments or a public file.
        $sql=if($role){"ALTER ROLE haruflow LOGIN PASSWORD '$appPassword';"}else{"CREATE ROLE haruflow LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD '$appPassword';"}
        $null=$sql | & (Join-Path $bin 'psql.exe') -X -w -v ON_ERROR_STOP=1 -f - 2>&1
        if($LASTEXITCODE -ne 0){throw 'Role privat gagal disiapkan; cluster dipertahankan.'}
        $owner=& (Join-Path $bin 'psql.exe') -X -w -Atq -v ON_ERROR_STOP=1 -c "SELECT pg_get_userbyid(datdba) FROM pg_database WHERE datname='haruflow';" 2>&1
        if($LASTEXITCODE -ne 0 -or ($owner -and $owner -ne 'haruflow')){throw 'Pemilik database bootstrap tidak sesuai; data dipertahankan.'}
        if(-not $owner){$null=& (Join-Path $bin 'psql.exe') -X -w -v ON_ERROR_STOP=1 -c 'CREATE DATABASE haruflow OWNER haruflow;' 2>&1;if($LASTEXITCODE -ne 0){throw 'Database privat gagal dibuat; cluster dipertahankan.'}}
        $env:PGUSER='haruflow';$env:PGPASSWORD=$appPassword;$env:PGDATABASE='haruflow'
        $ready=& (Join-Path $bin 'psql.exe') -X -w -Atq -v ON_ERROR_STOP=1 -c 'SELECT current_user;' 2>&1
        if($LASTEXITCODE -ne 0 -or $ready -ne 'haruflow'){throw 'Koneksi aplikasi privat belum siap; bootstrap dapat dicoba ulang.'}
        $config.bootstrap_pending=$false;Write-HaruflowPrivateJson (Join-Path $root 'postgres.json') $config
        Write-Output 'Cluster dan database privat siap. Kredensial tersimpan privat untuk proses setup.'
    }finally{Restore-HaruflowPgEnvironment $saved;$password=$null;$appPassword=$null;$sql=$null;if(Test-Path -LiteralPath $secretPath){Remove-Item -LiteralPath $secretPath -Force}}
    return
}
if(-not $config){if($Action -eq 'Status'){Write-Output 'Database memakai PostgreSQL eksternal; tidak ada cluster privat.';return};throw 'Tidak ada cluster privat milik instalasi ini.'}
$ctl=Join-Path $config.bin 'pg_ctl.exe';$data=$config.data_directory;$saved=Save-HaruflowPgEnvironment
try{
    $null=& $ctl status -D $data 2>&1;$running=$LASTEXITCODE -eq 0
    if($running){
        $pidFile=Join-Path $data 'postmaster.pid';$serverId=[int]([IO.File]::ReadAllLines($pidFile)[0]);$process=Get-CimInstance Win32_Process -Filter ('ProcessId='+$serverId)
        $expected=Join-Path $config.bin 'postgres.exe'
        if(-not $process -or $process.ExecutablePath -ne $expected -or $process.CommandLine.Replace('/','\') -notmatch ([regex]::Escape($data)+'(?:"|\s|$)')){throw 'Identitas proses PostgreSQL berbeda; proses tidak disentuh.'}
    }
    if($Action -eq 'Status'){Write-Output $(if($running){'PostgreSQL privat berjalan pada loopback port '+$config.port}else{'PostgreSQL privat berhenti.'});return}
    if($Action -eq 'Start' -and -not $running){
        # Windows PowerShell can wait forever on inherited native output pipes
        # after pg_ctl's child has detached. Use files and wait on pg_ctl only.
        $folder=Split-Path $data;$stamp=[Guid]::NewGuid().ToString('N');$stdout=Join-Path $folder ($stamp+'.start.stdout.log');$stderr=Join-Path $folder ($stamp+'.start.stderr.log')
        $arguments='start -D "'+$data+'" -l "'+(Join-Path $folder 'postgres.log')+'" -w -t 30'
        $launcher=Start-Process -FilePath $ctl -ArgumentList $arguments -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        if(-not $launcher.WaitForExit(45000)){throw 'PostgreSQL privat belum siap; periksa postgres/postgres.log.'}
        $null=& $ctl status -D $data 2>&1;if($LASTEXITCODE -ne 0){throw 'PostgreSQL privat gagal mulai; periksa postgres/postgres.log.'}
    }
    if($Action -eq 'Stop' -and $running){$null=& $ctl stop -D $data -m fast -w -t 30 2>&1;if($LASTEXITCODE -ne 0){throw 'PostgreSQL privat belum berhenti.'}}
}finally{Restore-HaruflowPgEnvironment $saved}
