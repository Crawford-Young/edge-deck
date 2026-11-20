//! Win32 side of the cursor restore: a low-level mouse hook on its own
//! thread feeds `Tracker`, and a thread timer moves the cursor. Only
//! `kind_of` is unit-tested; the hook acts on the real desktop, so it is
//! verified by live QA.

use std::cell::RefCell;
use std::sync::mpsc;

use ::windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Threading::GetCurrentThreadId;
use ::windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, GetMessageW, HC_ACTION, HHOOK, KillTimer, MSG, MSLLHOOKSTRUCT,
    PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetCursorPos, SetTimer, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_TIMER,
};

use super::plan::{Input, Kind, Point, RESTORE_DELAY_MS, Rect, Tracker, source_of};

/// The hook thread's state. The hook and the timer both run on that thread,
/// inside its `GetMessageW`, so a thread-local needs no lock.
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

/// A low-level mouse message as a `Kind`.
fn kind_of(message: u32) -> Kind {
    match message {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => Kind::Down,
        WM_LBUTTONUP | WM_RBUTTONUP => Kind::Up,
        _ => Kind::Move,
    }
}

/// The hook: classify, update the tracker, maybe arm the timer, pass on.
/// It never blocks, never moves the cursor and never swallows an event.
unsafe extern "system" fn on_mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to an MSLLHOOKSTRUCT that is
        // valid for the duration of this call.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let input = Input {
            source: source_of(info.dwExtraInfo),
            kind: kind_of(wparam.0 as u32),
            point: Point {
                x: info.pt.x,
                y: info.pt.y,
            },
        };
        // try_borrow: a panic must never unwind out of this callback.
        STATE.with(|cell| {
            if let Ok(mut state) = cell.try_borrow_mut()
                && let Some(state) = state.as_mut()
                && state.tracker.on_input(input)
            {
                state.arm();
            }
        });
    }
    // SAFETY: passes the event on unchanged; the hook handle is ignored.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// The timer fired: move the cursor back if the tracker says so.
fn restore() {
    let mut cursor = POINT::default();
    // SAFETY: writes the cursor position into a local POINT.
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return;
    }
    let target = STATE.with(|cell| {
        let mut state = cell.try_borrow_mut().ok()?;
        let state = state.as_mut()?;
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

/// Install the low-level mouse hook on the calling thread, which must pump
/// messages for it to be called.
fn install() -> ::windows::core::Result<HHOOK> {
    // SAFETY: the handle of this executable, which holds `on_mouse`.
    let module = unsafe { GetModuleHandleW(None) }?;
    // SAFETY: `on_mouse` matches HOOKPROC and lives as long as the process;
    // a low-level hook is called on the installing thread only.
    unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(on_mouse), Some(module.into()), 0) }
}

/// Install the hook, report this thread's id (or the error), then pump
/// messages until WM_QUIT.
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
    let hook = install();
    let hook = match hook {
        Ok(hook) => hook,
        Err(e) => {
            let _ = ready.send(Err(format!("mouse hook unavailable: {e}")));
            return;
        }
    };
    // SAFETY: reads the calling thread's id.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));
    // SAFETY: a standard message loop on this thread's own queue; 0 is
    // WM_QUIT and -1 an error, both end it.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        if msg.message == WM_TIMER {
            restore();
        }
    }
    // SAFETY: removes the hook this thread installed.
    let _ = unsafe { UnhookWindowsHookEx(hook) };
    STATE.with_borrow_mut(|state| {
        if let Some(state) = state.as_mut() {
            state.kill_timer();
        }
        *state = None;
    });
}

/// The running hook thread.
pub struct Restorer {
    thread_id: u32,
}

impl Restorer {
    /// Start the hook thread for an Edge at `edge`.
    pub fn start(edge: Rect) -> Result<Self, String> {
        let (ready, started) = mpsc::channel();
        std::thread::Builder::new()
            .name("cursor-restore".to_string())
            .spawn(move || run(edge, &ready))
            .map_err(|e| e.to_string())?;
        let thread_id = started
            .recv()
            .map_err(|_| "mouse hook thread ended".to_string())??;
        Ok(Restorer { thread_id })
    }

    /// Ask the hook thread to unhook and end.
    pub fn stop(&self) {
        // SAFETY: posts WM_QUIT to the hook thread's queue; fails harmlessly
        // if the thread is gone.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::windows::Win32::UI::WindowsAndMessaging::{
        WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_XBUTTONUP,
    };

    #[test]
    fn left_and_right_buttons_are_downs_and_ups_and_the_rest_moves() {
        assert_eq!(kind_of(WM_LBUTTONDOWN), Kind::Down);
        assert_eq!(kind_of(WM_RBUTTONDOWN), Kind::Down);
        assert_eq!(kind_of(WM_LBUTTONUP), Kind::Up);
        assert_eq!(kind_of(WM_RBUTTONUP), Kind::Up);
        for other in [WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MBUTTONDOWN, WM_XBUTTONUP] {
            assert_eq!(kind_of(other), Kind::Move);
        }
    }
}
