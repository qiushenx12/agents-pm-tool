; 安装包只装 Agents PM Tool 本身：不再随包分发 pm-cli，也不再改动用户 PATH。
;
; 这里保留一段**过渡期清理**：早期版本把 pm-cli.exe 装进应用目录、并把安装目录写进了
; 当前用户 PATH。升级到本版本的机器需要把这个残留项摘掉，否则会留下一个指向已消失
; 可执行文件的 PATH 项。
;
; 判据是「条目路径等于 $INSTDIR」，**不依赖旧版留下的 CliPathManaged 标记**：
; 实测存在「标记已经丢了、PATH 项还在」的机器（旧卸载器在标记不为 1 时不删项，
; 却无条件删掉标记），靠标记判断会直接漏掉。而指向安装目录的条目只可能来自本安装器
; ——pm-cli 已不在这里，这条 PATH 项对用户也没有任何用处。
;
; 待确认存量机器都已升级后，本文件可以直接删除、并从 tauri.conf.json 的
; bundle.windows.nsis.installerHooks 移除。

!include "WinMessages.nsh"

; 把 $INSTDIR 从当前用户 PATH 中摘掉（只删完全相等的那一项，其余原样保留）。
; 用 PowerShell 处理 PATH，避免 NSIS 1024 字符字符串上限截断已有内容。
; 结果写到 $0：0 表示成功（含「本来就没有」），非 0 表示失败。
!macro STRIP_INSTALL_DIR_FROM_USER_PATH
  System::Call 'Kernel32::SetEnvironmentVariable(t "AGENTS_PM_TOOL_INSTALL_DIR", t "$INSTDIR") i .r1'
  nsExec::ExecToLog `"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "try { $$d=($$env:AGENTS_PM_TOOL_INSTALL_DIR).TrimEnd([char]92); $$p=[Environment]::GetEnvironmentVariable('Path','User'); if ($$null -eq $$p) { exit 0 }; $$parts=@($$p -split ';' | Where-Object { $$_ -and $$_.Trim().TrimEnd([char]92) -ine $$d }); $$next=($$parts -join ';'); if ($$next -ne $$p) { [Environment]::SetEnvironmentVariable('Path',$$next,'User') }; exit 0 } catch { Write-Error $$_; exit 1 }"`
  Pop $0
  ${If} $0 = 0
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro STRIP_INSTALL_DIR_FROM_USER_PATH
  ${If} $0 = 0
    DetailPrint "已确保安装目录不在当前用户 PATH 中"
  ${Else}
    DetailPrint "警告：无法从当前用户 PATH 移除安装目录（PowerShell 退出码 $0）"
  ${EndIf}

  ; 旧版留下的标记值已经没有用了，顺手删掉。
  DeleteRegValue HKCU "${MANUPRODUCTKEY}" "CliPathManaged"

  ; 旧版随包安装的 pm-cli.exe 不再使用，清掉以免和新方式混用。
  IfFileExists "$INSTDIR\pm-cli.exe" 0 +2
    Delete "$INSTDIR\pm-cli.exe"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; 卸载同样保证不留下指向安装目录的 PATH 项（安装时已清，这里兜底）。
  !insertmacro STRIP_INSTALL_DIR_FROM_USER_PATH
!macroend
