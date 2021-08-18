#[cfg(target_family = "windows")]
use powershell_script;
#[cfg(not(target_os = "linux"))] // disable import for ubuntu
use log::{info, error};
#[cfg(not(target_os = "linux"))] // disable import for ubuntu
use std::env;

pub fn register_one_click() -> Result<String, String> {
    #[cfg(target_family = "windows")]
        {
            let mut command = r#"Start-Process powershell -Verb runAs -ArgumentList ""#.to_owned();
            command.push_str(env::current_exe().unwrap().to_str().unwrap());
            command.push_str(r#" --privileged-one-click""#);
            execute_command_with_logs(command.as_str())
        }
    #[cfg(target_os = "macos")]
        {
            use ::std::process::Command;
            let dir = env::current_dir().unwrap().clone();
            info!("Starting oneclick installation in directory {}", dir.display());
            match Command::new("./mac-install-oneclick.sh")
                .current_dir(dir)
                .spawn() {
                Ok(mut process) => {
                    if process.wait()
                        .map(|status| status.success())
                        .unwrap_or(false) {
                        info!("Installation succeeded");
                        Ok("Success".to_string())
                    } else {
                        error!("Setup script returned an error. Consider running it manually: ~/.aiosaber/client/mac-install-oneclick.sh");
                        Err("Setup script returned an error. Consider running it manually: ~/.aiosaber/client/mac-install-oneclick.sh".to_string())
                    }
                }
                Err(err) => {
                    error!("One-Click installer failed to start: {}", err);
                    let mut msg = "One-Click installer failed to start: ".to_string();
                    msg.push_str(err.to_string().as_str());
                    Err(msg)
                }
            }
        }

    #[cfg(target_os = "linux")]
        Err("Unsupported platform".to_string())
}

pub fn privileged_setup() {
    #[cfg(target_family = "windows")]
        {
            // reg add HKCR\aiosaber /v "OneClick-Provider" /d "AIOSaber"
            // reg add HKCR\aiosaber /v "URL Protocol"
            // reg add HKCR\aiosaber\shell\open\command /ve /d "PATH_TO_EXE --map-install %1"
            execute_command_with_logs(r#"reg add HKCR\aiosaber /f /v "OneClick-Provider" /d "AIOSaber""#)
                .expect("registry change #1 failed");
            execute_command_with_logs(r#"reg add HKCR\aiosaber /f /v "URL Protocol""#)
                .expect("registry change #2 failed");

            let mut command = r#"reg add HKCR\aiosaber\shell\open\command /f /ve /d ""#.to_owned();
            command.push_str(env::current_exe().unwrap().to_str().unwrap());
            command.push_str(r#" --map-install %1""#);
            execute_command_with_logs(command.as_str())
                .expect("registry change #3 failed");
        }
}

#[cfg(target_family = "windows")]
fn execute_command_with_logs(cmd: &str) -> Result<String, String> {
    match powershell_script::run(cmd, true) {
        Ok(output) => {
            if let Some(str) = output.stdout() {
                info!("OK: {}", str);
                Ok(str.to_string())
            } else if let Some(str) = output.stderr() {
                error!("Err: {}", str);
                Err(str.to_string())
            } else {
                info!("No output");
                Ok("".to_string())
            }
        }
        Err(error) => {
            error!("PsError: {}", error);
            Err(error.to_string())
        }
    }
}