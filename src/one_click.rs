#[cfg(target_family = "windows")]
use powershell_script;
use log::{info, error};
use std::env;
use std::io::Error;
use std::process::Child;

pub fn register_one_click() {
    #[cfg(target_family = "windows")]
        {
            let mut command = r#"Start-Process powershell -Verb runAs -ArgumentList ""#.to_owned();
            command.push_str(env::current_exe().unwrap().to_str().unwrap());
            command.push_str(r#" --privileged-one-click""#);
            execute_command_with_logs(command.as_str());
        }
    #[cfg(target_os = "macos")]
        {
            use::std::process::Command;
            let dir = env::current_dir().unwrap().clone();
            match Command::new("/bin/bash ./mac-install-oneclick.sh")
                .current_dir(dir)
                .spawn() {
                Ok(mut process) => {
                    if process.wait()
                        .map(|status| status.success())
                        .unwrap_or(false) {
                        info!("Installation succeeded");
                    } else {
                        error!("Setup script returned an error. Consider running it manually: ~/.aiosaber/client/mac-install-oneclick.sh");
                    }
                }
                Err(err) => {
                    error!("One-Click installer failed to start: {}", err);
                }
            }
        }
}

pub fn privileged_setup() {
    #[cfg(target_family = "windows")]
        {
            // reg add HKCR\aiosaber /v "OneClick-Provider" /d "AIOSaber"
            // reg add HKCR\aiosaber /v "URL Protocol"
            // reg add HKCR\aiosaber\shell\open\command /ve /d "PATH_TO_EXE --map-install %1"
            execute_command_with_logs(r#"reg add HKCR\aiosaber /f /v "OneClick-Provider" /d "AIOSaber""#);
            execute_command_with_logs(r#"reg add HKCR\aiosaber /f /v "URL Protocol""#);

            let mut command = r#"reg add HKCR\aiosaber\shell\open\command /f /ve /d ""#.to_owned();
            command.push_str(env::current_exe().unwrap().to_str().unwrap());
            command.push_str(r#" --map-install %1""#);
            execute_command_with_logs(command.as_str());
        }
}

#[cfg(target_family = "windows")]
fn execute_command_with_logs(cmd: &str) {
    match powershell_script::run(cmd, true) {
        Ok(output) => {
            if let Some(str) = output.stdout() {
                info!("OK: {}", str);
            } else if let Some(str) = output.stderr() {
                error!("Err: {}", str);
            } else {
                info!("No output");
            }
        }
        Err(error) => {
            error!("PsError: {}", error)
        }
    }
}