#!/bin/bash
mkdir ~/.aiosaber/client/
cp utility/serviceman-mac ~/.aiosaber/client/
cp aiosaber-client ~/.aiosaber/client/
cd ~/.aiosaber/client/ || exit
chmod +x serviceman-mac
chmod +x aiosaber-client
./serviceman-mac add --name "aiosaber-client" aiosaber-client