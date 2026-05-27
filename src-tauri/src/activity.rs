use std::{
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicPtr, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::db::Database;

const FLUSH_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(windows)]
const MOUSE_POLL_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Default)]
pub struct ActivityCounters {
    pending_keyboard: AtomicI64,
    pending_mouse: AtomicI64,
}

impl ActivityCounters {
    fn increment_keyboard(&self) {
        self.pending_keyboard.fetch_add(1, Ordering::Relaxed);
    }

    fn increment_mouse(&self) {
        self.pending_mouse.fetch_add(1, Ordering::Relaxed);
    }

    pub fn pending_counts(&self) -> (i64, i64) {
        (
            self.pending_keyboard.load(Ordering::Relaxed),
            self.pending_mouse.load(Ordering::Relaxed),
        )
    }

    fn take_pending_counts(&self) -> (i64, i64) {
        (
            self.pending_keyboard.swap(0, Ordering::Relaxed),
            self.pending_mouse.swap(0, Ordering::Relaxed),
        )
    }
}

pub struct ActivityHandle {
    counters: Arc<ActivityCounters>,
    stop: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
}

impl ActivityHandle {
    pub fn pending_counts(&self) -> (i64, i64) {
        self.counters.pending_counts()
    }

    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        #[cfg(windows)]
        wake_hook_thread();
        for handle in self.handles {
            if handle.join().is_err() {
                eprintln!("[StudyPulse activity] activity thread panicked while stopping");
            }
        }
    }
}

pub fn start_activity_capture(
    session_id: i64,
    db: Arc<Mutex<Database>>,
) -> Result<ActivityHandle, String> {
    start_platform_activity_capture(session_id, db)
}

fn flush_pending_counts(session_id: i64, db: &Arc<Mutex<Database>>, counters: &ActivityCounters) {
    let (keyboard, mouse) = counters.take_pending_counts();
    if keyboard == 0 && mouse == 0 {
        return;
    }

    match db.lock() {
        Ok(database) => {
            if keyboard > 0 {
                if let Err(error) = database.add_activity_event(session_id, "keyboard", keyboard) {
                    eprintln!("[StudyPulse activity] failed to save keyboard events: {error}");
                }
            }
            if mouse > 0 {
                if let Err(error) = database.add_activity_event(session_id, "mouse", mouse) {
                    eprintln!("[StudyPulse activity] failed to save mouse events: {error}");
                }
            }
        }
        Err(_) => eprintln!("[StudyPulse activity] database lock failed"),
    }
}

#[cfg(not(windows))]
fn start_platform_activity_capture(
    _session_id: i64,
    _db: Arc<Mutex<Database>>,
) -> Result<ActivityHandle, String> {
    Err("unsupported platform: activity capture only supports Windows".into())
}

#[cfg(windows)]
fn start_platform_activity_capture(
    session_id: i64,
    db: Arc<Mutex<Database>>,
) -> Result<ActivityHandle, String> {
    use windows::Win32::System::Threading::GetCurrentThreadId;

    let counters = Arc::new(ActivityCounters::default());
    let stop = Arc::new(AtomicBool::new(false));
    let keyboard_counters = Arc::clone(&counters);
    let keyboard_stop = Arc::clone(&stop);
    let mouse_counters = Arc::clone(&counters);
    let mouse_stop = Arc::clone(&stop);
    let mouse_db = Arc::clone(&db);

    let keyboard_handle = thread::spawn(move || {
        let thread_id = unsafe { GetCurrentThreadId() };
        set_hook_thread_id(thread_id);
        set_active_counters(Some(&keyboard_counters));

        if let Err(error) = run_keyboard_hook_loop(session_id, Arc::clone(&keyboard_stop)) {
            eprintln!("[StudyPulse activity] hook loop failed: {error}");
        }

        set_active_counters(None);
        clear_hook_thread_id();
    });
    let mouse_handle = thread::spawn(move || {
        run_mouse_polling_loop(session_id, mouse_db, mouse_counters, mouse_stop);
    });

    Ok(ActivityHandle {
        counters,
        stop,
        handles: vec![keyboard_handle, mouse_handle],
    })
}

#[cfg(windows)]
fn run_keyboard_hook_loop(session_id: i64, stop: Arc<AtomicBool>) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, MSG,
        WH_KEYBOARD_LL,
    };

    unsafe extern "system" fn keyboard_hook(
        code: i32,
        wparam: windows::Win32::Foundation::WPARAM,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        if code >= 0 && is_keyboard_down_message(wparam.0 as u32) {
            increment_active_keyboard();
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    let keyboard_hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0) }
        .map_err(|error| format!("keyboard hook install failed: {error}"))?;
    let hooks = InstalledHooks {
        keyboard: keyboard_hook,
    };

    println!("[StudyPulse activity] keyboard hook started for session {session_id}");
    while !stop.load(Ordering::SeqCst) {
        let mut message = MSG::default();
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 == -1 || result.0 == 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    drop(hooks);
    println!("[StudyPulse activity] keyboard hook stopped for session {session_id}");
    Ok(())
}

#[cfg(windows)]
fn run_mouse_polling_loop(
    session_id: i64,
    db: Arc<Mutex<Database>>,
    counters: Arc<ActivityCounters>,
    stop: Arc<AtomicBool>,
) {
    println!("[StudyPulse activity] mouse polling started for session {session_id}");
    let mut last_flush = Instant::now();
    let mut last_mouse_position = current_mouse_position();

    while !stop.load(Ordering::SeqCst) {
        thread::sleep(MOUSE_POLL_INTERVAL);

        let position = current_mouse_position();
        if mouse_position_changed(last_mouse_position, position) {
            counters.increment_mouse();
        }
        if position.is_some() {
            last_mouse_position = position;
        }

        if last_flush.elapsed() >= FLUSH_INTERVAL {
            flush_pending_counts(session_id, &db, &counters);
            last_flush = Instant::now();
        }
    }

    flush_pending_counts(session_id, &db, &counters);
    println!("[StudyPulse activity] mouse polling stopped for session {session_id}");
}

#[cfg(windows)]
struct InstalledHooks {
    keyboard: windows::Win32::UI::WindowsAndMessaging::HHOOK,
}

#[cfg(windows)]
impl Drop for InstalledHooks {
    fn drop(&mut self) {
        use windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx;

        unsafe {
            let _ = UnhookWindowsHookEx(self.keyboard);
        }
    }
}

#[cfg(windows)]
fn is_keyboard_down_message(message: u32) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{WM_KEYDOWN, WM_SYSKEYDOWN};
    message == WM_KEYDOWN || message == WM_SYSKEYDOWN
}

#[cfg(windows)]
fn current_mouse_position() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }
        .ok()
        .map(|_| (point.x, point.y))
}

#[cfg(windows)]
fn mouse_position_changed(previous: Option<(i32, i32)>, current: Option<(i32, i32)>) -> bool {
    previous
        .zip(current)
        .is_some_and(|(previous, current)| previous != current)
}

#[cfg(windows)]
static ACTIVE_COUNTERS: AtomicPtr<ActivityCounters> = AtomicPtr::new(std::ptr::null_mut());

#[cfg(windows)]
fn increment_active_keyboard() {
    let counters = ACTIVE_COUNTERS.load(Ordering::Relaxed);
    if !counters.is_null() {
        unsafe { (*counters).increment_keyboard() };
    }
}

#[cfg(windows)]
fn set_active_counters(counters: Option<&Arc<ActivityCounters>>) {
    let pointer = counters
        .map(|counters| Arc::as_ptr(counters) as *mut ActivityCounters)
        .unwrap_or(std::ptr::null_mut());
    ACTIVE_COUNTERS.store(pointer, Ordering::SeqCst);
}

#[cfg(windows)]
static HOOK_THREAD_ID: OnceLock<Mutex<Option<u32>>> = OnceLock::new();

#[cfg(windows)]
fn set_hook_thread_id(thread_id: u32) {
    if let Ok(mut value) = HOOK_THREAD_ID.get_or_init(|| Mutex::new(None)).lock() {
        *value = Some(thread_id);
    }
}

#[cfg(windows)]
fn clear_hook_thread_id() {
    if let Ok(mut value) = HOOK_THREAD_ID.get_or_init(|| Mutex::new(None)).lock() {
        *value = None;
    }
}

#[cfg(windows)]
fn wake_hook_thread() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_NULL};

    let thread_id = HOOK_THREAD_ID
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|value| *value);

    if let Some(thread_id) = thread_id {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_NULL, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn mouse_position_changes_are_counted_by_polling_logic() {
        assert!(mouse_position_changed(Some((10, 10)), Some((11, 10))));
        assert!(!mouse_position_changed(Some((10, 10)), Some((10, 10))));
    }

    #[test]
    fn missing_mouse_position_is_not_counted() {
        assert!(!mouse_position_changed(Some((10, 10)), None));
        assert!(!mouse_position_changed(None, Some((10, 10))));
    }
}
