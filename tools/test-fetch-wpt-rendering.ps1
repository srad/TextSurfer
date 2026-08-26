#requires -Version 7.0
$ErrorActionPreference = "Stop"

$root = Join-Path ([IO.Path]::GetTempPath()) "textsurfer-wpt-fetch-$([Guid]::NewGuid().ToString('N'))"
$source = Join-Path $root "source"
$destination = Join-Path $root "active\wpt"
$manifest = Join-Path $PSScriptRoot "wpt-rendering.json"
$fetch = Join-Path $PSScriptRoot "fetch-wpt-rendering.ps1"

try {
    New-Item -ItemType Directory -Path $source | Out-Null
    $selection = Get-Content -Raw -LiteralPath $manifest | ConvertFrom-Json
    $files = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $null = $files.Add("LICENSE.md")
    foreach ($case in @($selection.cases | Where-Object { $_.status -in @("run", "xfail") })) {
        $null = $files.Add([string]$case.path)
        foreach ($reference in @($case.references)) {
            $null = $files.Add([string]$reference.path)
        }
        foreach ($resource in @($case.resources)) {
            $null = $files.Add([string]$resource)
        }
    }
    foreach ($relative in $files) {
        $path = Join-Path $source ($relative -replace "/", [IO.Path]::DirectorySeparatorChar)
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
        $content = if ($relative -eq "LICENSE.md") {
            "Redistribution and use in source and binary forms"
        } else {
            "<!doctype html><p>$relative</p>"
        }
        Set-Content -LiteralPath $path -Value $content -NoNewline
    }
    foreach ($case in @($selection.cases | Where-Object { $_.status -in @("run", "xfail") })) {
        if ($case.kind -ne "reftest") {
            continue
        }
        $links = @($case.references | ForEach-Object {
            "<link rel='$($_.relation)' href='/$($_.path)'>"
        }) -join ""
        $path = Join-Path $source ($case.path -replace "/", [IO.Path]::DirectorySeparatorChar)
        Set-Content -LiteralPath $path -Value "<!doctype html>$links<p>$($case.path)</p>" -NoNewline
    }

    & $fetch -Manifest $manifest -Destination $destination -SourceRoot $source | Out-Null
    if (-not (Test-Path -LiteralPath (Join-Path $destination "SHA256SUMS"))) {
        throw "successful fetch did not activate the corpus"
    }
    Set-Content -LiteralPath (Join-Path $destination "sentinel") -Value "old" -NoNewline

    try {
        & $fetch -Manifest $manifest -Destination $destination -SourceRoot $source -TestFailAfter 2 | Out-Null
        throw "injected download failure unexpectedly succeeded"
    } catch {
        if ($_.Exception.Message -eq "injected download failure unexpectedly succeeded") {
            throw
        }
    }
    if ((Get-Content -Raw -LiteralPath (Join-Path $destination "sentinel")) -ne "old") {
        throw "download failure replaced the active corpus"
    }

    try {
        & $fetch -Manifest $manifest -Destination $destination -SourceRoot $source -TestFailActivation | Out-Null
        throw "injected activation failure unexpectedly succeeded"
    } catch {
        if ($_.Exception.Message -eq "injected activation failure unexpectedly succeeded") {
            throw
        }
    }
    if ((Get-Content -Raw -LiteralPath (Join-Path $destination "sentinel")) -ne "old") {
        throw "activation failure did not restore the active corpus"
    }

    $missing = Join-Path $source "LICENSE.md"
    Remove-Item -LiteralPath $missing -Force
    try {
        & $fetch -Manifest $manifest -Destination $destination -SourceRoot $source | Out-Null
        throw "missing source file unexpectedly succeeded"
    } catch {
        if ($_.Exception.Message -eq "missing source file unexpectedly succeeded") {
            throw
        }
    }
    if ((Get-Content -Raw -LiteralPath (Join-Path $destination "sentinel")) -ne "old") {
        throw "missing source file replaced the active corpus"
    }

    Write-Output "WPT rendering fetch tests passed"
} finally {
    if (Test-Path -LiteralPath $root) {
        Remove-Item -LiteralPath $root -Recurse -Force
    }
}
