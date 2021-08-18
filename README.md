# AIOSaber

## The Name

AIO is meant to be an acronym of "All In One", while "Saber" represents the game which this tool is made for.

It is mean to be AIO, since it combines Map & Mod Management for one or multiple installations of Beat Saber, even
across multiple operating systems in a single place. And it does support *both*: PC & Quest.

## Architecture

Since this client is supposed to auto-update maps to the latest version & keep mods up to date and eventually do other
background tasks, this client runs as a daemon / background service.

Since the client is made in Rust, you dont have to worry about the applications footprint, it takes less memory than an
average windows process and uses almost no CPU. In fact it is idling most of the time, if you don't have the 
configurator open.

## Configuration

As of now, there is no real configuration interface (because I'm shit at web dev), but in future there will be a proper
interface with all functionalities scoped for AIOSaber.

Temporary configuration of AIOSaber: [https://aiosaber.zerotwo.workers.dev/](https://aiosaber.zerotwo.workers.dev/)

# But... What does it do?

## Features

Well, right now it doesn't do much, just yet.

You can download maps to your pc and/or your quest from Windows/MacOS via OneClick-Installer.

**To get the OneClick-Install Button on BeatSaver check [this](https://github.com/AIOSaber/BeatSaver-Extension)**
