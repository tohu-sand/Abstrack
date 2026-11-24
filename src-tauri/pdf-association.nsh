!define PDF_PROGID "Abstrack.PDF"

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "Software\Classes\${PDF_PROGID}" "" "PDF Document"
  WriteRegStr SHCTX "Software\Classes\${PDF_PROGID}\DefaultIcon" "" '"$INSTDIR\${MAINBINARYNAME}.exe",0'
  WriteRegStr SHCTX "Software\Classes\${PDF_PROGID}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
  WriteRegStr SHCTX "Software\Classes\.pdf\OpenWithProgids" "${PDF_PROGID}" ""
  !insertmacro UPDATEFILEASSOC
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue SHCTX "Software\Classes\.pdf\OpenWithProgids" "${PDF_PROGID}"
  DeleteRegKey SHCTX "Software\Classes\${PDF_PROGID}"
  !insertmacro UPDATEFILEASSOC
!macroend
