; TopNote installer customisations, included near the top of Tauri's NSIS template.

; ---- Page text --------------------------------------------------------------
!define MUI_WELCOMEPAGE_TITLE "Welcome to TopNote"
!define MUI_WELCOMEPAGE_TEXT "TopNote puts live, synced Spotify lyrics in a small window that floats above everything else on your screen.$\r$\n$\r$\nThis will install TopNote ${VERSION} for your Windows account. No admin rights needed.$\r$\n$\r$\nClick Next to continue."

!define MUI_FINISHPAGE_TITLE "TopNote is ready"
; Keep this short: the run/shortcut checkboxes sit about five lines below the title.
!define MUI_FINISHPAGE_TEXT "Play something on Spotify and the lyrics will follow along.$\r$\n$\r$\nPress Ctrl+Alt+M to show or hide TopNote, and right-click it for every option."
!define MUI_FINISHPAGE_RUN_TEXT "Launch TopNote now"

; ---- Uninstall cleanup -------------------------------------------------------
; Tauri already removes %APPDATA%\lb.imad.topnote when "delete app data" is ticked.
; TopNote also keeps the Spotify sign-in and sp_dc cookie in Windows Credential
; Manager, and older builds used a different data folder, so clear those too.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    nsExec::Exec 'cmdkey /delete:refresh_token.topnote'
    Pop $0
    nsExec::Exec 'cmdkey /delete:sp_dc.topnote'
    Pop $0
    nsExec::Exec 'cmdkey /delete:refresh_token.spotify-miniplayer'
    Pop $0
    nsExec::Exec 'cmdkey /delete:sp_dc.spotify-miniplayer'
    Pop $0
    SetShellVarContext current
    RmDir /r "$APPDATA\lb.imad.spotify-miniplayer"
    RmDir /r "$APPDATA\lb.imad.spotify-miniplayer.migrated"
    RmDir /r "$LOCALAPPDATA\lb.imad.spotify-miniplayer"
  ${EndIf}
!macroend
