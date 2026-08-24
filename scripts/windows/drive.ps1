<#
.SYNOPSIS
    Press the Windows desktop from one JSON list of actions.

.DESCRIPTION
    Runs in session 1 (see session1.ps1) and carries out a sequence of operations
    read from a JSON array: move and click the mouse, send keys, front or maximize
    a window, capture the screen, list what is running. Each step prints one line
    saying what it did, so the log read back on the other machine is a transcript
    of the run rather than a silence.

    THE WHOLE SEQUENCE MUST BE ONE PLAN. A second run cannot continue where the
    first left off: starting it brings something else to the front, and what was on
    screen does not survive that. A webview's context menu closes on `blur`, and a
    dialog with no owner window — `SHOpenWithDialog` is one — disappears without a
    trace. Open the menu, press the item, and shoot the result in a single plan.

    The screen is the only reliable witness. A dialog put up by the modern shell
    does not appear in the `GW_HWNDNEXT` chain the `z` op walks, so its absence
    there says nothing; shoot and look.

.PARAMETER Plan
    Path to the JSON file. One array of objects, each with an `op`.

.PARAMETER OutDir
    Where a `shot` with a relative path is written. Absolute paths are left alone.

.NOTES
    The ops, and what each object carries besides `op`:

      move      x, y                  put the pointer there
      click     x, y                  left click there
      dblclick  x, y                  double click there
      rclick    x, y                  right click there
      type      text                  send it as keystrokes (SendKeys syntax)
      key       text                  the same door, named for single keys ("{ENTER}")
      sleep     ms                    wait
      shot      path                  capture every screen to a PNG
      procs     -                     every process that has a window, with its pid
      z         -                     top-level windows front to back (see above)
      focus     pid | name            restore and front that process's main window
      maximize  pid | name            maximize and front it
      run       path, args            start a program, report its pid

    `focus` and `maximize` take either — a `pid` when a previous run already reported
    it, or a `name` (the process name, no .exe) resolved at the step itself. The name
    is what a plan that starts the program uses, since a pid the same plan just
    printed cannot be written into it beforehand.

    `pid` is a property on the action object, never a variable: `$pid` is read-only
    in PowerShell. `op` is spelled `op` and not `do` for the same class of reason —
    `do` is a keyword, so `$a.do` does not parse.
#>
param(
    [Parameter(Mandatory = $true)][string]$Plan,
    [string]$OutDir = "$env:USERPROFILE\amenbo-drive"
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# CharSet.Unicode on GetWindowTextW is not optional: without it the marshaller
# passes an ANSI buffer and every title comes back one character long.
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class AmenboU {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint c);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static string Title(IntPtr h) { var sb = new StringBuilder(512); GetWindowTextW(h, sb, 512); return sb.ToString(); }
}
"@

$MOUSE_LEFTDOWN = 0x0002
$MOUSE_LEFTUP = 0x0004
$MOUSE_RIGHTDOWN = 0x0008
$MOUSE_RIGHTUP = 0x0010
$SW_MAXIMIZE = 3
$SW_RESTORE = 9
$GW_HWNDNEXT = 2

function Resolve-ShotPath([string]$path) {
    if ([System.IO.Path]::IsPathRooted($path)) { return $path }
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
    return (Join-Path $OutDir $path)
}

# Every screen at once, in the desktop's own coordinates — which is what the x/y in
# a plan are, so a shot and a click always agree about where a thing is.
function Save-Shot([string]$path) {
    $target = Resolve-ShotPath $path
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($bounds.X, $bounds.Y, 0, 0, $bmp.Size)
    $bmp.Save($target, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    "shot -> $target"
}

# Visible top-level windows, front to back, walked from the foreground one.
function Get-ZOrder() {
    $out = @()
    $cur = [AmenboU]::GetForegroundWindow()
    for ($i = 0; $i -lt 40 -and $cur -ne [IntPtr]::Zero; $i++) {
        if ([AmenboU]::IsWindowVisible($cur)) {
            $title = [AmenboU]::Title($cur)
            if ($title) {
                $wpid = 0
                [void][AmenboU]::GetWindowThreadProcessId($cur, [ref]$wpid)
                $r = New-Object AmenboU+RECT
                [void][AmenboU]::GetWindowRect($cur, [ref]$r)
                $out += ("{0}  pid={1}  [{2},{3} {4}x{5}]  hwnd={6}" -f `
                        $title, $wpid, $r.L, $r.T, ($r.R - $r.L), ($r.B - $r.T), $cur)
            }
        }
        $cur = [AmenboU]::GetWindow($cur, $GW_HWNDNEXT)
    }
    $out
}

# The window an action names, by pid or by process name. A name matching several
# processes takes the first that has a window, since the rest have nothing to front.
function Get-MainWindow($action) {
    $found = if ($action.pid) { Get-Process -Id $action.pid -ErrorAction SilentlyContinue }
    elseif ($action.name) { Get-Process -Name $action.name -ErrorAction SilentlyContinue }
    else { $null }
    foreach ($p in @($found)) {
        if ($p -and $p.MainWindowHandle -ne 0) { return $p.MainWindowHandle }
    }
    return [IntPtr]::Zero
}

# What a focus/maximize step was aiming at, for its line in the transcript.
function Get-Target($action) {
    if ($action.pid) { return "pid=$($action.pid)" }
    if ($action.name) { return "name=$($action.name)" }
    return "nothing"
}

# The parameter is $Plan; the parsed content must not reuse that name. Assigning
# ConvertFrom-Json's result over a [string] parameter would coerce the objects back
# to a string — and PowerShell's variable names are case-insensitive, so $plan is
# the same variable as $Plan.
$steps = Get-Content $Plan -Raw -Encoding utf8 | ConvertFrom-Json

foreach ($a in $steps) {
    switch ($a.op) {
        "move" {
            [void][AmenboU]::SetCursorPos($a.x, $a.y)
            "move $($a.x),$($a.y)"
        }
        "click" {
            [void][AmenboU]::SetCursorPos($a.x, $a.y); Start-Sleep -Milliseconds 120
            [AmenboU]::mouse_event($MOUSE_LEFTDOWN, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 60
            [AmenboU]::mouse_event($MOUSE_LEFTUP, 0, 0, 0, [IntPtr]::Zero)
            "click $($a.x),$($a.y)"
        }
        "dblclick" {
            [void][AmenboU]::SetCursorPos($a.x, $a.y); Start-Sleep -Milliseconds 120
            foreach ($i in 1..2) {
                [AmenboU]::mouse_event($MOUSE_LEFTDOWN, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 40
                [AmenboU]::mouse_event($MOUSE_LEFTUP, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 60
            }
            "dblclick $($a.x),$($a.y)"
        }
        "rclick" {
            [void][AmenboU]::SetCursorPos($a.x, $a.y); Start-Sleep -Milliseconds 120
            [AmenboU]::mouse_event($MOUSE_RIGHTDOWN, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 60
            [AmenboU]::mouse_event($MOUSE_RIGHTUP, 0, 0, 0, [IntPtr]::Zero)
            "rclick $($a.x),$($a.y)"
        }
        "type" { [System.Windows.Forms.SendKeys]::SendWait($a.text); "type" }
        "key" { [System.Windows.Forms.SendKeys]::SendWait($a.text); "key $($a.text)" }
        "sleep" { Start-Sleep -Milliseconds $a.ms; "sleep $($a.ms)" }
        "shot" { Save-Shot $a.path }
        "z" { "--- z-order ---"; Get-ZOrder }
        "procs" {
            Get-Process | Where-Object { $_.MainWindowTitle } |
                ForEach-Object { "{0} pid={1} : {2}" -f $_.ProcessName, $_.Id, $_.MainWindowTitle }
        }
        "focus" {
            $h = Get-MainWindow $a
            if ($h -ne [IntPtr]::Zero) {
                [void][AmenboU]::ShowWindow($h, $SW_RESTORE)
                [void][AmenboU]::SetForegroundWindow($h)
                "focus $(Get-Target $a)"
            } else { "focus: no window for $(Get-Target $a)" }
        }
        "maximize" {
            $h = Get-MainWindow $a
            if ($h -ne [IntPtr]::Zero) {
                [void][AmenboU]::ShowWindow($h, $SW_MAXIMIZE); Start-Sleep -Milliseconds 400
                [void][AmenboU]::SetForegroundWindow($h)
                "maximize $(Get-Target $a)"
            } else { "maximize: no window for $(Get-Target $a)" }
        }
        "run" {
            $started = if ($a.args) { Start-Process -FilePath $a.path -ArgumentList $a.args -PassThru }
            else { Start-Process -FilePath $a.path -PassThru }
            "run $($a.path) -> pid=$($started.Id)"
        }
        default { "unknown op: $($a.op)" }
    }
}
