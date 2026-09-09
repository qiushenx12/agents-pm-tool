; 将随安装包发布的 pm-cli.exe 注册到当前用户 PATH。
; 使用 PowerShell 处理 PATH，避免 NSIS 1024 字符字符串上限截断已有内容。

!include "WinMessages.nsh"

!macro NSIS_HOOK_POSTINSTALL
  ; 通过临时进程环境变量传递安装目录，避免路径中的空格影响命令解析。
  System::Call 'Kernel32::SetEnvironmentVariable(t "AGENTS_PM_TOOL_INSTALL_DIR", t "$INSTDIR") i .r1'
  nsExec::ExecToLog `"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "try { $$d=$$env:AGENTS_PM_TOOL_INSTALL_DIR; $$p=[Environment]::GetEnvironmentVariable('Path','User'); if ($$null -eq $$p) { $$p='' }; $$normalized=$$d.TrimEnd([char]92); $$exists=@($$p -split ';' | Where-Object { $$_ -and $$_.Trim().TrimEnd([char]92) -ieq $$normalized }).Count -gt 0; if ($$exists) { exit 10 }; if ([string]::IsNullOrWhiteSpace($$p)) { $$next=$$d } else { $$next=$$p.TrimEnd(';')+';'+$$d }; [Environment]::SetEnvironmentVariable('Path',$$next,'User'); exit 0 } catch { Write-Error $$_; exit 1 }"`
  Pop $0

  ${If} $0 = 0
    ; 只在本安装器实际添加 PATH 时记录，避免卸载时删除用户原有配置。
    WriteRegDWORD HKCU "${MANUPRODUCTKEY}" "CliPathManaged" 1
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
    DetailPrint "已将 pm-cli 添加到当前用户 PATH"
  ${ElseIf} $0 = 10
    WriteRegDWORD HKCU "${MANUPRODUCTKEY}" "CliPathManaged" 0
    DetailPrint "pm-cli 所在目录已在当前用户 PATH 中"
  ${Else}
    WriteRegDWORD HKCU "${MANUPRODUCTKEY}" "CliPathManaged" 0
    DetailPrint "警告：无法将 pm-cli 添加到当前用户 PATH（PowerShell 退出码 $0）"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $0 0
  ReadRegDWORD $0 HKCU "${MANUPRODUCTKEY}" "CliPathManaged"
  ${If} $0 = 1
    System::Call 'Kernel32::SetEnvironmentVariable(t "AGENTS_PM_TOOL_INSTALL_DIR", t "$INSTDIR") i .r1'
    nsExec::ExecToLog `"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "try { $$d=($$env:AGENTS_PM_TOOL_INSTALL_DIR).TrimEnd([char]92); $$p=[Environment]::GetEnvironmentVariable('Path','User'); if ($$null -eq $$p) { exit 0 }; $$parts=@($$p -split ';' | Where-Object { $$_ -and $$_.Trim().TrimEnd([char]92) -ine $$d }); [Environment]::SetEnvironmentVariable('Path',($$parts -join ';'),'User'); exit 0 } catch { Write-Error $$_; exit 1 }"`
    Pop $0
    ${If} $0 = 0
      SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
      DetailPrint "已从当前用户 PATH 移除 pm-cli 所在目录"
    ${Else}
      DetailPrint "警告：无法从当前用户 PATH 移除 pm-cli 所在目录（PowerShell 退出码 $0）"
    ${EndIf}
  ${EndIf}
  DeleteRegValue HKCU "${MANUPRODUCTKEY}" "CliPathManaged"
!macroend
