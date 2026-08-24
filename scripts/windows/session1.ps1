<#
.SYNOPSIS
    Run a PowerShell script in the interactive desktop session, from a shell that is not in it.

.DESCRIPTION
    An `ssh` session on Windows lands in session 0, elevated. Nothing drawn there is
    on a screen, so a script that opens a window, presses a button, or reads a dialog
    cannot be run from there at all. The only way across is to hand the work to the
    Task Scheduler with `-LogonType Interactive`, which starts it in the logged-on
    user's own session — session 1, not elevated, with that user's real PATH.

    This registers such a task, starts it, waits for the script to finish, prints
    whatever it wrote, and unregisters the task again. Output is collected through a
    file rather than a pipe, because the task's process is not this one's child and
    there is no stream back from it.

    Two details are not decoration. `-WindowStyle Hidden` keeps the task's own
    console off the screen: a console that appears takes the foreground, and a
    dialog without an owner window (`SHOpenWithDialog` is one) closes silently the
    moment something else comes to the front. And the principal is named by the
    current account's SID, not by `DOMAIN\user` — the latter fails with
    "No mapping between account names and security IDs" on a local account whose
    USERDOMAIN is the machine name.

.PARAMETER Script
    Path, on this machine, to the script to run in session 1.

.PARAMETER TimeoutSec
    How long to wait for it. On expiry the task is unregistered and TIMEOUT is
    reported; whatever the script wrote up to that point is still printed.

.PARAMETER WorkDir
    Where the wrapper, the log and the completion marker are written.

.PARAMETER TaskName
    The scheduled task's name. Distinct names let two runs coexist; each one only
    ever unregisters its own.

.EXAMPLE
    powershell -File session1.ps1 -Script C:\Users\me\amenbo-drive\plan-run.ps1
#>
param(
    [Parameter(Mandatory = $true)][string]$Script,
    [int]$TimeoutSec = 120,
    [string]$WorkDir = "$env:USERPROFILE\amenbo-drive",
    [string]$TaskName = "amenbo-session1"
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
$log = Join-Path $WorkDir "$TaskName.log"
$doneMark = Join-Path $WorkDir "$TaskName.done"
$wrapper = Join-Path $WorkDir "$TaskName-wrap.ps1"
Remove-Item $log, $doneMark -Force -ErrorAction SilentlyContinue

# The wrapper is what the task actually runs. It exists to guarantee the completion
# marker: without it, a script that throws would leave the waiter below to sit out
# the full timeout for a run that ended in the first second.
@"
`$ProgressPreference = 'SilentlyContinue'
try { & '$Script' *>&1 | Out-File -FilePath '$log' -Encoding utf8 }
catch { 'ERR: ' + `$_.Exception.Message | Out-File -FilePath '$log' -Encoding utf8 -Append }
finally { 'done' | Out-File -FilePath '$doneMark' -Encoding utf8 }
"@ | Out-File -FilePath $wrapper -Encoding utf8

$sid = ([Security.Principal.WindowsIdentity]::GetCurrent()).User.Value
try {
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue
    $action = New-ScheduledTaskAction -Execute "powershell.exe" `
        -Argument "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$wrapper`""
    $principal = New-ScheduledTaskPrincipal -UserId $sid -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -ExecutionTimeLimit ([TimeSpan]::FromSeconds([Math]::Max($TimeoutSec * 2, 120)))
    Register-ScheduledTask -TaskName $TaskName -Action $action -Principal $principal -Settings $settings | Out-Null
    Start-ScheduledTask -TaskName $TaskName
} catch {
    "REGISTER FAILED: " + $_.Exception.Message
    return
}

$deadline = (Get-Date).AddSeconds($TimeoutSec)
while (-not (Test-Path $doneMark) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 400 }
$timedOut = -not (Test-Path $doneMark)

Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue
if (Test-Path $log) { Get-Content $log -Encoding utf8 }
if ($timedOut) { "TIMEOUT after $TimeoutSec s" }
