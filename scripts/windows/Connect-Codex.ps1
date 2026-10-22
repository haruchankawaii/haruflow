[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$HaruflowExe,
    [string]$ServerUrl = 'http://127.0.0.1:8080',
    [string]$TokenFile = (Join-Path $env:LOCALAPPDATA 'HaruflowCommunity\ai\codex-token.txt')
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Gunakan panduan MCP.md untuk sistem lain.' }
if ($ServerUrl -notmatch '^http://(127\.0\.0\.1|localhost|\[::1\])(:[0-9]{1,5})?/?$') { throw 'Alamat harus HTTP loopback pada PC ini, tanpa path, kredensial atau query.' }
$uri = [uri]$ServerUrl
if ($uri.Port -le 0 -or $uri.Port -gt 65535) { throw 'Port server tidak valid.' }
$program = (Resolve-Path -LiteralPath $HaruflowExe).Path
if (-not (Test-Path -LiteralPath $program -PathType Leaf)) { throw 'Program Haruflow tidak ditemukan.' }
$path = [IO.Path]::GetFullPath($TokenFile)
if (Test-Path -LiteralPath $path) { throw 'Berkas kunci sudah ada. Cabut kunci lama di Haruflow dan pilih -TokenFile baru untuk rotasi.' }
$folder = [IO.Path]::GetDirectoryName($path)
[IO.Directory]::CreateDirectory($folder) | Out-Null
# Reject existing reparse points rather than writing a credential through a link.
$ancestor = $folder
while ($ancestor) {
    if ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Folder kunci tidak boleh memakai symlink/junction.' }
    $parent = [IO.Path]::GetDirectoryName($ancestor)
    if ($parent -eq $ancestor) { break }
    $ancestor = $parent
}
$secure = Read-Host 'Tempel kunci pribadi dari Pengaturan > Integrasi AI (disembunyikan)' -AsSecureString
$pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
try {
    $token = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
    if ($token -cnotmatch '^hf_mcp_[0-9a-f]{64}$') { throw 'Format kunci Haruflow tidak valid.' }
    # Create empty first, then restrict access before writing any secret bytes.
    $stream = [IO.File]::Open($path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    $stream.Dispose()
    $acl = [Security.AccessControl.FileSecurity]::new()
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl.SetOwner($sid)
    $acl.SetAccessRuleProtection($true,$false)
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow'))
    Set-Acl -LiteralPath $path -AclObject $acl
    [IO.File]::WriteAllText($path,$token,[Text.UTF8Encoding]::new($false))
} finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
    $token = $null
    $secure.Dispose()
}
# JSON escaping is valid for these TOML basic strings, including Windows paths.
$exeLiteral = ConvertTo-Json -InputObject $program -Compress
$urlLiteral = ConvertTo-Json -InputObject $ServerUrl.TrimEnd('/') -Compress
$fileLiteral = ConvertTo-Json -InputObject $path -Compress
Write-Output 'Kunci disimpan untuk akun Windows ini. Tambahkan konfigurasi berikut ke Codex, lalu restart koneksi MCP:'
Write-Output @"
[mcp_servers.haruflow]
command = $exeLiteral
args = ["mcp"]

[mcp_servers.haruflow.env]
HARUFLOW_MCP_URL = $urlLiteral
HARUFLOW_MCP_TOKEN_FILE = $fileLiteral
"@
Write-Output 'Skrip tidak mengubah pengaturan Codex. Panduan verifikasi: https://github.com/haruchankawaii/haruflow/blob/main/docs/MCP.md'
