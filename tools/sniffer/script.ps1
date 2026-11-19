Get-ChildItem dump_*.bin | Sort-Object Name | Select-Object -First 20 | ForEach-Object {
    $bytes = [IO.File]::ReadAllBytes($_.FullName)
    $hex = ($bytes | Select-Object -First 256 | ForEach-Object { $_.ToString("x2") }) -join " "
    $ascii = -join ($bytes | Select-Object -First 256 | ForEach-Object { if ($_ -ge 32 -and $_ -le 126) { [char]$_ } else { "." } })
    "=== $($_.Name) ($($bytes.Length) bytes) ===`r`nHEX:   $hex`r`nASCII: $ascii`r`n"
} | Out-File packets_preview.txt -Encoding utf8