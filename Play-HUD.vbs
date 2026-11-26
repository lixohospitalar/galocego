Option Explicit

Dim shell, files, root, game, exe, env, command
Set shell = CreateObject("WScript.Shell")
Set files = CreateObject("Scripting.FileSystemObject")
root = files.GetParentFolderName(WScript.ScriptFullName)

If WScript.Arguments.Count < 1 Or WScript.Arguments.Count > 2 Then
    MsgBox "Arraste o executavel do seu jogo para Play-HUD.vbs.", vbInformation, "Galocego"
    WScript.Quit 2
End If

game = files.GetAbsolutePathName(WScript.Arguments(0))
exe = files.BuildPath(root, "target\diag\victory.exe")
If WScript.Arguments.Count = 2 Then exe = files.GetAbsolutePathName(WScript.Arguments(1))
If Not files.FileExists(exe) Or Not files.FileExists(game) Then
    MsgBox "Verifique os caminhos do Victory e do jogo.", vbExclamation, "Galocego"
    WScript.Quit 2
End If

Set env = shell.Environment("PROCESS")
env("GALOCEGO_LAUNCH_GAME") = game
env("GALOCEGO_LAUNCH_EXE") = exe
env("VICTORY_HUD") = "1"
If env("VICTORY_LOG") = "" Then env("VICTORY_LOG") = "info"
shell.CurrentDirectory = root
command = "$ErrorActionPreference='Stop'; " & _
    "$dir=Join-Path $env:LOCALAPPDATA ('Galocego\Logs\'+[Guid]::NewGuid()); " & _
    "$null=New-Item -ItemType Directory -Path $dir -Force; " & _
    "$game=[char]34+$env:GALOCEGO_LAUNCH_GAME+[char]34; " & _
    "$p=Start-Process -FilePath $env:GALOCEGO_LAUNCH_EXE -ArgumentList @('run',$game,'--execute') " & _
    "-WindowStyle Hidden -RedirectStandardOutput (Join-Path $dir 'stdout.log') " & _
    "-RedirectStandardError (Join-Path $dir 'stderr.log') -Wait -PassThru; exit $p.ExitCode"
shell.Run "powershell.exe -NoProfile -NonInteractive -WindowStyle Hidden -Command " & Chr(34) & command & Chr(34), 0, False
