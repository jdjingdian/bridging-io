param(
  [switch]$SkipCoreSelfTest
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$rustRoot = Join-Path $repoRoot "source\rust"

Push-Location $rustRoot
try {
  Write-Host "[preflight] windows ssh broker contract: bridgingio-secrets named-pipe runtime unit"
  cargo test -p bridgingio-secrets windows_broker_runtime_exposes_named_pipe_contract_without_identity_fallback -- --nocapture
  if ($LASTEXITCODE -ne 0) {
    throw "bridgingio-secrets windows named-pipe runtime unit failed with exit code $LASTEXITCODE"
  }

  Write-Host "[preflight] windows ssh broker contract: bridgingio-core vault/auth smoke"
  cargo test -p bridgingio-mcp --bin bridgingio-core vault_and_auth_contract_smoke -- --nocapture
  if ($LASTEXITCODE -ne 0) {
    throw "bridgingio-core vault/auth smoke failed with exit code $LASTEXITCODE"
  }

  if (-not $SkipCoreSelfTest) {
    Write-Host "[preflight] windows ssh broker contract: bridgingio-core --self-test"
    cargo run -p bridgingio-mcp --bin bridgingio-core -- --self-test
    if ($LASTEXITCODE -ne 0) {
      throw "bridgingio-core --self-test failed with exit code $LASTEXITCODE"
    }
  }
} finally {
  Pop-Location
}
