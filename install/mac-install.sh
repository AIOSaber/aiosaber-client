#!/bin/bash
mkdir -p ~/.aiosaber/client/
cp utility/serviceman-mac ~/.aiosaber/client/
cp aiosaber-client ~/.aiosaber/client/
cp mac-install-oneclick.sh ~/.aiosaber/client/
cd ~/.aiosaber/client/ || exit
chmod +x serviceman-mac
chmod +x aiosaber-client
echo "The necessary files have been placed, yet the two binaries required for running aiosaber needs to be approved."
echo "I might properly sign those at later times, but not for now."
echo "Read: https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unidentified-developer-mh40616/mac"
echo "Once you're ready to do that, press ENTER"
read -r
echo "Please approve 'serviceman' - It's used to manage daemon jobs across multiple operating systems"
./serviceman-mac
echo "Once it is approved, please press ENTER again"
read -r
echo "Please approve 'aiosaber-client' - It's the daemon binary for AIOSaber"
./aiosaber-client --dry-run
echo "Once it is approved please press ENTER again to setup the daemon service"
read -r
./serviceman-mac add --name "aiosaber-client" ./aiosaber-client
echo "The necessary steps have been completed, in case something didn't work, because you didn't approve the binaries:"
echo "You can just rerun this installer!"