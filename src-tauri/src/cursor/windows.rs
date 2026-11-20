//! Win32 side of the cursor restore, all on one thread: a low-level mouse
//! hook gives the real mouse's position, a hidden message-only window
//! receives raw input from the touchscreen, and a thread timer moves the
//! cursor once the touchscreen goes quiet. A touch on the webview reaches no
//! mouse hook, so only raw input hears it. This acts on the real desktop, so
//! it is verified by live QA; every decision is `plan::Tracker`.

use std::cell::RefCell;
use std::sync::mpsc;

use ::windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Threading::GetCurrentThreadId;
use ::windows::Win32::UI::Input::{
    RAWINPUTDEVICE, RAWINPUTDEVICE_FLAGS, RIDEV_INPUTSINK, RIDEV_REMOVE, RegisterRawInputDevices,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos,
    GetMessageW, HC_ACTION, HHOOK, HWND_MESSAGE, KillTimer, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE,
    PeekMessageW, PostThreadMessageW, RegisterClassW, SetCursorPos, SetTimer, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_MOUSE_LL, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT, WM_QUIT, WM_TIMER,
    WNDCLASSW,
};
use ::windows::core::w;

use super::plan::{Input, Point, RESTORE_DELAY_MS, Rect, Tracker, source_of};

/// HID usage page and usage of a touch screen (Digitizers, Touch Screen).
const DIGITIZER_PAGE: u16 = 0x0D;
const TOUCH_SCREEN: u16 = 0x04;

/// The hook thread's state. The hook, the window and the timer all run on
/// that thread, inside its `GetMessageW`, so a thread-local needs no lock.
struct HookState {
    tracker: Tracker,
    /// The running restore timer's id, 0 when none.
    timer: usize,
}

impl HookState {
    fn kill_timer(&mut self) {
        if self.timer != 0 {
            // SAFETY: kills a thread timer this thread created; a stale id
            // only fails.
            let _ = unsafe { KillTimer(None, self.timer) };
            self.timer = 0;
        }
    }

    /// (Re)start the restore timer. A thread timer gets a fresh id each
    /// time, so the old one is killed first.
    fn arm(&mut self) {
        self.kill_timer();
        // SAFETY: a thread timer with no callback; it posts WM_TIMER to this
        // thread's queue.
        self.timer = unsafe { SetTimer(None, 0, RESTORE_DELAY_MS, None) };
    }
}

thread_local! {
    static STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

/// Update the tracker and maybe (re)arm the timer. `try_borrow`: a panic
/// must never unwind out of a Win32 callback.
fn feed(input: Input) {
    STATE.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut()
            && let Some(state) = state.as_mut()
            && state.tracker.on_input(input)
        {
            state.arm();
        }
    });
}

/// The hook: classify, feed, pass on. It never blocks, never moves the
/// cursor and never swallows an event.
unsafe extern "system" fn on_mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to an MSLLHOOKSTRUCT that is
        // valid for the duration of this call.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        feed(Input::Hook {
            source: source_of(info.dwExtraInfo),
            point: Point {
                x: info.pt.x,
                y: info.pt.y,
            },
        });
    }
    // SAFETY: passes the event on unchanged; the hook handle is ignored.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// The message-only window: every WM_INPUT is touch activity. The report
/// itself is not read; DefWindowProcW frees it.
unsafe extern "system" fn on_window(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_INPUT {
        feed(Input::TouchReport);
    }
    // SAFETY: default handling of this window's own message.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// A WM_TIMER for `id` arrived: if it is the running restore timer, move the
/// cursor back when the tracker says so. A stale one (killed after it was
/// queued) does nothing.
fn on_timer(id: usize) {
    let mut cursor = POINT::default();
    // SAFETY: writes the cursor position into a local POINT.
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return;
    }
    let target = STATE.with(|cell| {
        let mut state = cell.try_borrow_mut().ok()?;
        let state = state.as_mut()?;
        if id != state.timer {
            return None;
        }
        state.kill_timer();
        state.tracker.due(Point {
            x: cursor.x,
            y: cursor.y,
        })
    });
    // Moved with the state released: should the move reach the hook on this
    // thread, it reads as a mouse event at the target, which changes nothing.
    if let Some(p) = target {
        // SAFETY: plain cursor move; no memory is passed.
        let _ = unsafe { SetCursorPos(p.x, p.y) };
    }
}

/// The touchscreen raw-input registration, sent to `target` even while
/// another window has focus; with `RIDEV_REMOVE` and no target, its removal.
fn touch_device(flags: RAWINPUTDEVICE_FLAGS, target: HWND) -> [RAWINPUTDEVICE; 1] {
    [RAWINPUTDEVICE {
        usUsagePage: DIGITIZER_PAGE,
        usUsage: TOUCH_SCREEN,
        dwFlags: flags,
        hwndTarget: target,
    }]
}

/// Install the hook and create the raw-input window on the calling thread,
/// which must pump messages for either to be called.
fn install() -> Result<(HHOOK, HWND), String> {
    // SAFETY: the handle of this executable, which holds the callbacks.
    let module = unsafe { GetModuleHandleW(None) }.map_err(|e| format!("no module handle: {e}"))?;
    let class = w!("edge-deck-touch-listener");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(on_window),
        hInstance: module.into(),
        lpszClassName: class,
        ..Default::default()
    };
    // SAFETY: `wc` and its static class name outlive the call; `on_window`
    // matches WNDPROC and lives as long as the process.
    if unsafe { RegisterClassW(&wc) } == 0 {
        return Err(format!(
            "touch window class unavailable: {}",
            ::windows::core::Error::from_thread()
        ));
    }
    // SAFETY: a message-only window (no size, never shown) of the class
    // just registered, owned by this thread.
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(module.into()),
            None,
        )
    }
    .map_err(|e| format!("touch window unavailable: {e}"))?;
    let device = touch_device(RIDEV_INPUTSINK, hwnd);
    // SAFETY: one valid RAWINPUTDEVICE whose target is this thread's window.
    let registered =
        unsafe { RegisterRawInputDevices(&device, std::mem::size_of::<RAWINPUTDEVICE>() as u32) };
    // SAFETY: `on_mouse` matches HOOKPROC and lives as long as the process;
    // a low-level hook is called on the installing thread only.
    let hook = registered.and_then(|()| unsafe {
        SetWindowsHookExW(WH_MOUSE_LL, Some(on_mouse), Some(module.into()), 0)
    });
    match hook {
        Ok(hook) => Ok((hook, hwnd)),
        Err(e) => {
            uninstall(None, hwnd);
            Err(format!("touch listener unavailable: {e}"))
        }
    }
}

/// Undo `install`: unhook, drop the raw-input registration, destroy the
/// window. Each step only fails if it was never done.
fn uninstall(hook: Option<HHOOK>, hwnd: HWND) {
    if let Some(hook) = hook {
        // SAFETY: removes the hook this thread installed.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }
    let device = touch_device(RIDEV_REMOVE, HWND::default());
    // SAFETY: one valid RAWINPUTDEVICE asking to remove this process's
    // touchscreen registration.
    let _ =
        unsafe { RegisterRawInputDevices(&device, std::mem::size_of::<RAWINPUTDEVICE>() as u32) };
    // SAFETY: destroys the window this thread created.
    let _ = unsafe { DestroyWindow(hwnd) };
}

/// Install, report this thread's id (or the error), then pump messages
/// until WM_QUIT.
fn run(edge: Rect, ready: &mpsc::Sender<Result<u32, String>>) {
    STATE.with_borrow_mut(|state| {
        *state = Some(HookState {
            tracker: Tracker::new(edge),
            timer: 0,
        });
    });
    let mut msg = MSG::default();
    // SAFETY: creates this thread's message queue so a WM_QUIT posted as
    // soon as `start` returns is not lost; removes nothing.
    let _ = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE) };
    let (hook, hwnd) = match install() {
        Ok(installed) => installed,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    // SAFETY: reads the calling thread's id.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
    // SAFETY: a standard message loop on this thread's own queue; 0 is
    // WM_QUIT and -1 an error, both end it.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        if msg.message == WM_TIMER && msg.hwnd.is_invalid() {
            on_timer(msg.wParam.0);
        } else {
            // SAFETY: hands a message this thread received to its window.
            unsafe { DispatchMessageW(&msg) };
        }
    }
    uninstall(Some(hook), hwnd);
    STATE.with_borrow_mut(|state| {
        if let Some(state) = state.as_mut() {
            state.kill_timer();
        }
        *state = None;
    });
}

/// The running listener thread.
pub struct Restorer {
    thread_id: u32,
}

impl Restorer {
    /// Start the listener thread for an Edge at `edge`.
    pub fn start(edge: Rect) -> Result<Self, String> {
        let (ready, started) = mpsc::channel();
        std::thread::Builder::new()
            .name("cursor-restore".to_string())
            .spawn(move || run(edge, &ready))
            .map_err(|e| e.to_string())?;
        let thread_id = started
            .recv()
            .map_err(|_| "cursor restore thread ended".to_string())??;
        Ok(Restorer { thread_id })
    }

    /// Ask the listener thread to clean up and end.
    pub fn stop(&self) {
        // SAFETY: posts WM_QUIT to the thread's queue; fails harmlessly if
        // the thread is gone.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
    }
}
