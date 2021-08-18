mkdir "%USERPROFILE%\.aiosaber\client\"
copy windows-uninstall.bat "%USERPROFILE%\.aiosaber\client\"
copy utility\serviceman-win.exe "%USERPROFILE%\.aiosaber\client\"
copy aiosaber-client.exe "%USERPROFILE%\.aiosaber\client\"
cd "%USERPROFILE%\.aiosaber\client\"
serviceman-win.exe add --name "aiosaber-client" aiosaber-client.exe