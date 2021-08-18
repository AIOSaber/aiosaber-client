cd utility
serviceman-win.exe stop aiosaber-client
cd ..
taskkill /f /im serviceman.aiosaber-client.exe
taskkill /f /im aiosaber-client.exe
copy aiosaber-client.exe "%USERPROFILE%\.aiosaber\client\"
cd "%USERPROFILE%\.aiosaber\client\"
serviceman-win.exe start aiosaber-client