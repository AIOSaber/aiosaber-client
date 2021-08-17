#!/bin/bash
cd ~/.aiosaber/client/ || exit
wget https://github.com/AIOSaber/uri-handler/archive/refs/heads/master.zip
unzip master.zip
rm master.zip
mv uri-handler-master uri-handler
cd uri-handler || exit
./setup