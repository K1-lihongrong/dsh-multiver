
$out = "E:\.LLM-AGENTS\cuckoo-code-\deepseek-Desktop\dsh-multiver\.ci-poll3.log"
Remove-Item $out -ErrorAction SilentlyContinue
for ($i = 1; $i -le 25; $i++) {
  try {
    $runs = Invoke-RestMethod -Uri "https://api.github.com/repos/K1-lihongrong/dsh-multiver/actions/runs?per_page=5" -Headers @{ "User-Agent" = "poll" } -TimeoutSec 20
    $latest = $runs.workflow_runs | Where-Object { $_.head_branch -eq "v0.2.4-rc.3" } | Select-Object -First 1
    if ($latest) {
      Add-Content $out ("[{0}] {1} status={2} concl={3}" -f $i, $latest.head_branch, $latest.status, $latest.conclusion)
      if ($latest.status -eq "completed") { break }
    } else {
      Add-Content $out ("[{0}] 未找到 rc.3 run" -f $i)
    }
  } catch { Add-Content $out ("[{0}] ERR: {1}" -f $i, $_) }
  Start-Sleep -Seconds 60
}
