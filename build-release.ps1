# Builds PeriCore.exe, then PeriCore-Setup.exe (which embeds it), into dist\.
# Exit codes are checked after each step (cargo writes progress to stderr).
Set-Location $PSScriptRoot

cargo test --release
if ($LASTEXITCODE -ne 0) { throw 'tests failed' }
cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'PeriCore build failed' }
cargo build --release --manifest-path tools/installer/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw 'installer build failed' }

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\PeriCore.exe dist\PeriCore.exe -Force
Copy-Item tools\installer\target\release\PeriCore-Setup.exe "dist\PeriCore-Setup-$version.exe" -Force
Get-ChildItem dist | Format-Table Name, Length
