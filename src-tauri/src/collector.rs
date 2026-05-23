use chrono::Utc;

#[derive(Debug, Clone)]
pub struct WindowSample {
    pub app_name: String,
    pub window_title: String,
    pub exe_path: Option<String>,
    pub sampled_at: String,
}

pub fn sample_foreground_window() -> Result<WindowSample, String> {
    sample_platform_foreground_window()
}

#[cfg(not(windows))]
fn sample_platform_foreground_window() -> Result<WindowSample, String> {
    Err("unsupported platform: foreground window collection only supports Windows".into())
}

#[cfg(windows)]
fn sample_platform_foreground_window() -> Result<WindowSample, String> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::Path};
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{CloseHandle, MAX_PATH},
            System::Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
            UI::WindowsAndMessaging::{
                GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
            },
        },
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return Err("no foreground window".into());
        }

        let title_len = GetWindowTextLengthW(hwnd);
        let mut title_buffer = vec![0u16; (title_len + 1).max(1) as usize];
        let title_read = GetWindowTextW(hwnd, &mut title_buffer);
        let window_title = if title_read > 0 {
            OsString::from_wide(&title_buffer[..title_read as usize])
                .to_string_lossy()
                .to_string()
        } else {
            "Untitled window".into()
        };

        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        let exe_path = if process_id == 0 {
            None
        } else {
            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) {
                Ok(process) => {
                    let mut path_buffer = vec![0u16; MAX_PATH as usize * 4];
                    let mut size = path_buffer.len() as u32;
                    let result = QueryFullProcessImageNameW(
                        process,
                        PROCESS_NAME_FORMAT(0),
                        PWSTR(path_buffer.as_mut_ptr()),
                        &mut size,
                    );
                    let _ = CloseHandle(process);

                    if result.is_ok() {
                        Some(
                            OsString::from_wide(&path_buffer[..size as usize])
                                .to_string_lossy()
                                .to_string(),
                        )
                    } else {
                        None
                    }
                }
                Err(error) => {
                    eprintln!("[StudyPulse collector] OpenProcess failed: {error}");
                    None
                }
            }
        };

        let app_name = exe_path
            .as_ref()
            .and_then(|path| Path::new(path).file_stem())
            .map(|name| name.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Unknown app".into());

        Ok(WindowSample {
            app_name,
            window_title,
            exe_path,
            sampled_at: Utc::now().to_rfc3339(),
        })
    }
}
