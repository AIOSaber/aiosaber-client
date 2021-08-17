#[cfg(target_env = "msvc")]
use powershell_script;
use log::{info, error};
use std::env;

pub fn register_one_click() {
    #[cfg(target_env = "msvc")]
        {
            let mut command = r#"Start-Process powershell -Verb runAs -ArgumentList ""#.to_owned();
            command.push_str(env::current_exe().unwrap().to_str().unwrap());
            command.push_str(r#" --privileged-one-click""#);
            execute_command_with_logs(command.as_str());
        }
}

pub fn privileged_setup() {
    #[cfg(target_env = "msvc")]
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

#[cfg(target_env = "msvc")]
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