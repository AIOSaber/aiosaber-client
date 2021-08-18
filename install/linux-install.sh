#!/bin/bash
mkdir -p ~/.aiosaber/client/
cp utility/serviceman-linux ~/.aiosaber/client/
cp aiosaber-client ~/.aiosaber/client/
cd ~/.aiosaber/client/ || exit
chmod +x serviceman-linux
chmod +x aiosaber-client
./serviceman-linux add --name "aiosaber-client" aiosaber-client