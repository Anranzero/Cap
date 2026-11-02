# 每次改完自动同步到 GitHub（Anranzero/Cap）
#
# 用法：
#   pwsh -NoProfile -File sync.ps1                    # 自动生成提交信息
#   pwsh -NoProfile -File sync.ps1 -Message "改了xxx"  # 指定提交信息
#   pwsh -NoProfile -File sync.ps1 --no-push          # 只提交，不推
param(
    [string]$Message = "",
    [switch]$NoPush
)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot

git add -A
$staged = (git diff --cached --name-only | Measure-Object).Count
if ($staged -eq 0) {
    Write-Output "没有需要提交的改动。"
    exit 0
}

if (-not $Message) {
    $branch = git branch --show-current
    $stamp = Get-Date -Format 'yyyy-MM-dd HH:mm'
    $Message = "sync: $stamp（$branch，$staged 个文件）"
}

git commit -m $Message | Out-Host
if ($NoPush) {
    Write-Output "已提交（未推送）。"
    exit 0
}

$env:GIT_TERMINAL_PROMPT = '0'
$env:GCM_INTERACTIVE = 'never'
git push | Out-Host
Write-Output "已同步到 origin/main。"
