#requires -Version 7.0
param(
    [string]$Commit = "ed37f83e79426194b075cfc29a5454894ae6f36f"
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$out = Join-Path $root "testdata"
New-Item -ItemType Directory -Force -Path $out | Out-Null

$headers = @{ "User-Agent" = "textsurf-fetch-corpus" }
$base = "https://raw.githubusercontent.com/web-platform-tests/wpt/$Commit/html/syntax/parsing/resources"
$api = "https://api.github.com/repos/web-platform-tests/wpt/contents/html/syntax/parsing/resources?ref=$Commit"

$listing = Invoke-RestMethod -Headers $headers $api
$files = @($listing | Where-Object { $_.name -like "*.dat" } | ForEach-Object { $_.name } | Sort-Object)

if ($files.Count -eq 0) {
    throw "no .dat files found at pinned commit $Commit"
}

$manifest = @()
$downloaded = 0
$skipped = 0
foreach ($name in $files) {
    $target = Join-Path $out $name
    $currentHash = if (Test-Path -LiteralPath $target) {
        (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
    } else { $null }
    $tmp = Join-Path $env:TEMP "$name.$PID.tmp"
    try {
        Invoke-WebRequest -UseBasicParsing -Headers $headers -OutFile $tmp "$base/$name"
        $hash = (Get-FileHash -LiteralPath $tmp -Algorithm SHA256).Hash
        if ($currentHash -eq $hash) {
            $skipped++
        } else {
            Move-Item -Force -LiteralPath $tmp -Destination $target
            $downloaded++
        }
        $manifest += "$hash  $name"
    } finally {
        Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath $tmp
    }
}

Set-Content -LiteralPath (Join-Path $out "wpt-parsing.sha256") -Value $manifest -Encoding ascii
Set-Content -LiteralPath (Join-Path $out "wpt-parsing.pin") -Value $Commit -Encoding ascii

Write-Output "commit: $Commit"
Write-Output "files: $($files.Count) ($downloaded downloaded, $skipped up-to-date)"
Write-Output "manifest: $(Join-Path $out 'wpt-parsing.sha256')"