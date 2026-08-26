#requires -Version 7.0
param(
    [string]$Manifest = (Join-Path $PSScriptRoot "wpt-rendering.json"),
    [string]$Destination = (Join-Path $PSScriptRoot "..\testdata\wpt"),
    [string]$SourceRoot = "",
    [int]$TestFailAfter = -1,
    [switch]$TestFailActivation
)

$ErrorActionPreference = "Stop"

$manifestPath = (Resolve-Path -LiteralPath $Manifest).Path
$selection = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($selection.schema -ne 1) {
    throw "unsupported WPT rendering manifest schema"
}
if ($selection.upstream.commit -notmatch "^[0-9a-f]{40}$") {
    throw "WPT rendering manifest has an invalid commit"
}

$destinationPath = [IO.Path]::GetFullPath($Destination)
$destinationParent = [IO.Path]::GetFullPath((Split-Path -Parent $destinationPath))
if ([IO.Path]::GetFileName($destinationPath) -ne "wpt" -or $destinationParent -eq [IO.Path]::GetPathRoot($destinationParent)) {
    throw "destination must be an explicitly named wpt directory below a parent"
}
New-Item -ItemType Directory -Force -Path $destinationParent | Out-Null

$runCases = @($selection.cases | Where-Object { $_.status -in @("run", "xfail") })
$files = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$null = $files.Add("LICENSE.md")
foreach ($case in $runCases) {
    $null = $files.Add([string]$case.path)
    foreach ($reference in @($case.references)) {
        $null = $files.Add([string]$reference.path)
    }
    foreach ($resource in @($case.resources)) {
        $null = $files.Add([string]$resource)
    }
}
$relativeFiles = @($files | Sort-Object)

foreach ($relative in $relativeFiles) {
    if ([IO.Path]::IsPathRooted($relative) -or $relative.Contains("\") -or $relative.Contains(":") -or $relative.Contains("?") -or $relative.Contains("#") -or $relative.Split("/") -contains "..") {
        throw "unsafe corpus path: $relative"
    }
}

$token = [Guid]::NewGuid().ToString("N")
$stage = Join-Path $destinationParent ".wpt-stage-$token"
$backup = Join-Path $destinationParent ".wpt-backup-$token"
$activated = $false
$movedOld = $false

try {
    New-Item -ItemType Directory -Path $stage | Out-Null
    $headers = @{ "User-Agent" = "textsurfer-fetch-wpt-rendering" }
    $base = "https://raw.githubusercontent.com/web-platform-tests/wpt/$($selection.upstream.commit)"
    $downloaded = 0
    foreach ($relative in $relativeFiles) {
        $target = Join-Path $stage ($relative -replace "/", [IO.Path]::DirectorySeparatorChar)
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
        if ($SourceRoot) {
            $source = Join-Path ([IO.Path]::GetFullPath($SourceRoot)) ($relative -replace "/", [IO.Path]::DirectorySeparatorChar)
            Copy-Item -LiteralPath $source -Destination $target
        } else {
            Invoke-WebRequest -UseBasicParsing -Headers $headers -OutFile $target "$base/$relative"
        }
        $downloaded++
        if ($TestFailAfter -ge 0 -and $downloaded -ge $TestFailAfter) {
            throw "injected download failure"
        }
    }

    $hashes = foreach ($relative in $relativeFiles) {
        $path = Join-Path $stage ($relative -replace "/", [IO.Path]::DirectorySeparatorChar)
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "missing staged file: $relative"
        }
        "$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant())  $relative"
    }
    Set-Content -LiteralPath (Join-Path $stage "SHA256SUMS") -Value $hashes -Encoding ascii

    $actual = @(Get-ChildItem -LiteralPath $stage -Recurse -File | ForEach-Object {
        [IO.Path]::GetRelativePath($stage, $_.FullName).Replace("\", "/")
    } | Sort-Object)
    $expected = @($relativeFiles + "SHA256SUMS" | Sort-Object)
    if (Compare-Object -ReferenceObject $expected -DifferenceObject $actual) {
        throw "staged corpus membership differs from the manifest"
    }

    $repository = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
    $previousManifest = $env:TEXTSURFER_WPT_MANIFEST
    $previousCorpus = $env:TEXTSURFER_WPT_CORPUS_ROOT
    try {
        $env:TEXTSURFER_WPT_MANIFEST = $manifestPath
        $env:TEXTSURFER_WPT_CORPUS_ROOT = $stage
        Push-Location $repository
        try {
            cargo test --test wpt_rendering -- --exact manifest_contracts
            if ($LASTEXITCODE -ne 0) {
                throw "typed WPT manifest validation failed"
            }
            cargo test --test wpt_rendering -- --exact corpus_integrity
            if ($LASTEXITCODE -ne 0) {
                throw "staged WPT corpus validation failed"
            }
        } finally {
            Pop-Location
        }
    } finally {
        if ($null -eq $previousManifest) {
            Remove-Item Env:TEXTSURFER_WPT_MANIFEST -ErrorAction SilentlyContinue
        } else {
            $env:TEXTSURFER_WPT_MANIFEST = $previousManifest
        }
        if ($null -eq $previousCorpus) {
            Remove-Item Env:TEXTSURFER_WPT_CORPUS_ROOT -ErrorAction SilentlyContinue
        } else {
            $env:TEXTSURFER_WPT_CORPUS_ROOT = $previousCorpus
        }
    }

    if (Test-Path -LiteralPath $destinationPath) {
        Move-Item -LiteralPath $destinationPath -Destination $backup
        $movedOld = $true
    }
    if ($TestFailActivation) {
        throw "injected activation failure"
    }
    Move-Item -LiteralPath $stage -Destination $destinationPath
    $activated = $true
    if ($movedOld) {
        Remove-Item -LiteralPath $backup -Recurse -Force
    }
    Write-Output "commit: $($selection.upstream.commit)"
    Write-Output "cases: $($runCases.Count)"
    Write-Output "files: $($relativeFiles.Count)"
    Write-Output "corpus: $destinationPath"
} catch {
    if (-not $activated -and $movedOld -and -not (Test-Path -LiteralPath $destinationPath) -and (Test-Path -LiteralPath $backup)) {
        Move-Item -LiteralPath $backup -Destination $destinationPath
    }
    throw
} finally {
    if (Test-Path -LiteralPath $stage) {
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
    if ($activated -and (Test-Path -LiteralPath $backup)) {
        Remove-Item -LiteralPath $backup -Recurse -Force
    }
}
