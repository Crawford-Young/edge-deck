//! Win32 + UI Automation implementation of `Desktop`. Not unit-tested: it acts
//! on the real desktop, so it is verified by live QA only.

use std::ffi::c_void;
use std::process::Command;

use ::windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
use ::windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use ::windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use ::windows::Win32::System::Variant::VARIANT;
use ::windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationSelectionItemPattern,
    TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_SelectionItemPatternId,
    UIA_TabItemControlTypeId,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use ::windows::core::BOOL;

use super::Desktop;
use super::plan::{ProcInfo, WindowInfo};

fn fail(what: &'static str) -> impl Fn(::windows::core::Error) -> String {
    move |e| format!("{what}: {e}")
}

/// Keeps COM initialised on this thread for as long as it lives.
struct ComThread {
    uninit: bool,
}

impl ComThread {
    fn init() -> Self {
        // SAFETY: plain COM initialisation of the current thread.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        // A thread already in another apartment can still use UIA; it just
        // must not be uninitialised by us.
        ComThread { uninit: hr.is_ok() }
    }
}

impl Drop for ComThread {
    fn drop(&mut self) {
        if self.uninit {
            // SAFETY: balances the successful CoInitializeEx in `init`.
            unsafe { CoUninitialize() };
        }
    }
}

pub struct Win32Desktop {
    // Field order matters: the UIA client must be released before COM is.
    uia: IUIAutomation,
    _com: ComThread,
}

impl Win32Desktop {
    pub fn new() -> Result<Self, String> {
        let com = ComThread::init();
        // SAFETY: COM is initialised on this thread by `com`.
        let uia: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .map_err(fail("UI Automation unavailable"))?;
        Ok(Win32Desktop { uia, _com: com })
    }

    /// The window's `TabItem` elements in UIA tree order (the tab strip order).
    fn tab_items(&self, hwnd: HWND) -> Result<Vec<IUIAutomationElement>, String> {
        let read = fail("reading terminal tabs");
        // SAFETY: UIA calls on a live client; a stale hwnd only yields an error.
        unsafe {
            let root = self.uia.ElementFromHandle(hwnd).map_err(&read)?;
            let condition = self
                .uia
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_TabItemControlTypeId.0),
                )
                .map_err(&read)?;
            let found = root
                .FindAll(TreeScope_Descendants, &condition)
                .map_err(&read)?;
            (0..found.Length().map_err(&read)?)
                .map(|i| found.GetElement(i).map_err(&read))
                .collect()
        }
    }
}

fn to_hwnd(hwnd: isize) -> HWND {
    HWND(hwnd as *mut c_void)
}

/// Restore the window if minimised and make it the foreground window.
fn raise(hwnd: HWND) -> Result<(), String> {
    // SAFETY: these calls accept any hwnd and report failure by return value.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() {
            Ok(())
        } else {
            Err("Windows did not bring the terminal to the front".to_string())
        }
    }
}

struct Collect {
    pid: u32,
    hwnds: Vec<HWND>,
}

/// EnumWindows callback: keep visible, unowned top-level windows of `pid`.
unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Collect` passed by `windows_of`, alive for
    // the whole synchronous EnumWindows call.
    let state = unsafe { &mut *(lparam.0 as *mut Collect) };
    let mut owner_pid = 0u32;
    // SAFETY: hwnd comes from EnumWindows; the out pointer is a live local.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut owner_pid));
        let unowned = GetWindow(hwnd, GW_OWNER).is_err();
        if owner_pid == state.pid && IsWindowVisible(hwnd).as_bool() && unowned {
            state.hwnds.push(hwnd);
        }
    }
    true.into()
}

impl Desktop for Win32Desktop {
    fn processes(&self) -> Result<Vec<ProcInfo>, String> {
        let snap = fail("listing processes");
        // SAFETY: the snapshot handle is closed below; the entry is sized.
        unsafe {
            let handle: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(&snap)?;
            let mut entry = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut procs = Vec::new();
            let mut more = Process32FirstW(handle, &mut entry).is_ok();
            while more {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                procs.push(ProcInfo {
                    pid: entry.th32ProcessID,
                    parent_pid: entry.th32ParentProcessID,
                    exe: String::from_utf16_lossy(&entry.szExeFile[..len]),
                });
                more = Process32NextW(handle, &mut entry).is_ok();
            }
            let _ = CloseHandle(handle);
            Ok(procs)
        }
    }

    fn windows_of(&self, pid: u32) -> Result<Vec<WindowInfo>, String> {
        let mut state = Collect {
            pid,
            hwnds: Vec::new(),
        };
        // SAFETY: the callback only dereferences `state`, which outlives the call.
        unsafe {
            EnumWindows(
                Some(collect_window),
                LPARAM(&mut state as *mut Collect as isize),
            )
        }
        .map_err(fail("listing terminal windows"))?;
        state
            .hwnds
            .into_iter()
            .map(|hwnd| {
                let tabs = self
                    .tab_items(hwnd)?
                    .iter()
                    // SAFETY: reading a property of a live UIA element.
                    .map(|tab| unsafe { tab.CurrentName() }.map(|n| n.to_string()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(fail("reading tab names"))?;
                Ok(WindowInfo {
                    hwnd: hwnd.0 as isize,
                    pid,
                    tabs,
                })
            })
            .collect()
    }

    fn select_tab(&self, hwnd: isize, tab: usize) -> Result<(), String> {
        let hwnd = to_hwnd(hwnd);
        let tabs = self.tab_items(hwnd)?;
        let item = tabs.get(tab).ok_or("the terminal tab has closed")?;
        // SAFETY: UIA calls on a live element.
        unsafe {
            item.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                UIA_SelectionItemPatternId,
            )
            .and_then(|pattern| pattern.Select())
        }
        .map_err(fail("selecting the terminal tab"))?;
        raise(hwnd)
    }

    fn raise_windows(&self, hwnds: &[isize]) -> Result<(), String> {
        // EnumWindows lists front-to-back; raising back-to-front keeps the
        // frontmost window on top.
        hwnds
            .iter()
            .rev()
            .try_for_each(|&hwnd| raise(to_hwnd(hwnd)))
    }

    fn spawn(&self, program: &str, args: &[String]) -> Result<(), String> {
        // The child is detached on purpose: the new terminal outlives this call.
        Command::new(program)
            .args(args)
            .spawn()
            .map(drop)
            .map_err(|e| format!("opening a terminal: {e}"))
    }
}
