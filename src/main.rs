// Trio500FnFix (Rust): makes the Samsung Smart Keyboard Trio 500 top row act as F1-F12 without holding Fn.
//
// Without Fn the keyboard sends HID consumer-control usages for F1-F6 and F10-F12 (captured with
// tools/trio500-keylog.ps1). This tray app listens for those reports through Raw Input and injects the
// matching F-key. F7-F9 are the keyboard's Bluetooth slot keys and send nothing to the PC without Fn.
// Tray menu: Pause, Run at Windows startup (HKCU\...\CurrentVersion\Run), Exit.
//
// Build: cargo build --release   (no external crates; the Win32 API is declared by hand below)
#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering::Relaxed};

const DEVICE_FILTER: &str = "VID&0204E8_PID&A064";

// Consumer usage sent without Fn -> scancode of the F-key printed on that key.
const FKEY_SCAN: [(u16, u16); 9] = [
    (0x0071, 0x3B), // F1
    (0x0072, 0x3C), // F2
    (0x0073, 0x3D), // F3
    (0x0302, 0x3E), // F4
    (0x0301, 0x3F), // F5
    (0x0223, 0x40), // F6 (AC Home)
    (0x0070, 0x44), // F10
    (0x006F, 0x57), // F11
    (0x006E, 0x58), // F12
];

fn scan_for(usage: u16) -> Option<u16> {
    FKEY_SCAN.iter().find(|&&(u, _)| u == usage).map(|&(_, s)| s)
}

// ---- Win32 declarations -------------------------------------------------------------------

type WndProc = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;
type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;

const WM_DESTROY: u32 = 0x0002;
const WM_NULL: u32 = 0x0000;
const WM_INPUT: u32 = 0x00FF;
const WM_TIMER: u32 = 0x0113;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_CONTEXTMENU: u32 = 0x007B;
const WM_TRAY: u32 = 0x8001; // WM_APP + 1
const RIDEV_INPUTSINK: u32 = 0x0100;
const RID_INPUT: u32 = 0x1000_0003;
const RIDI_PREPARSEDDATA: u32 = 0x2000_0005;
const RIDI_DEVICENAME: u32 = 0x2000_0007;
const RIM_TYPEHID: u32 = 2;
const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;
const WH_KEYBOARD_LL: i32 = 13;
const VK_BROWSER_HOME: u32 = 0xAC;
const INPUT_KEYBOARD: u32 = 1;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const KEYEVENTF_SCANCODE: u32 = 0x0008;
const SPI_GETKEYBOARDSPEED: u32 = 0x000A;
const SPI_GETKEYBOARDDELAY: u32 = 0x0016;
const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIF_MESSAGE: u32 = 0x01;
const NIF_ICON: u32 = 0x02;
const NIF_TIP: u32 = 0x04;
const NIF_INFO: u32 = 0x10;
const NIIF_INFO: u32 = 0x01;
const MF_STRING: u32 = 0x0000;
const MF_CHECKED: u32 = 0x0008;
const MF_SEPARATOR: u32 = 0x0800;
const TPM_RIGHTBUTTON: u32 = 0x0002;
const TPM_NONOTIFY: u32 = 0x0080;
const TPM_RETURNCMD: u32 = 0x0100;
const DT_CENTER: u32 = 0x01;
const DT_VCENTER: u32 = 0x04;
const DT_SINGLELINE: u32 = 0x20;
const ERROR_ALREADY_EXISTS: u32 = 183;
const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
const RRF_RT_REG_SZ: u32 = 0x0002;
const REG_SZ: u32 = 1;

#[repr(C)]
struct WNDCLASSEXW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: Option<WndProc>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: isize,
    h_icon: isize,
    h_cursor: isize,
    hbr_background: isize,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: isize,
}

#[repr(C)]
#[derive(Default)]
struct POINT {
    x: i32,
    y: i32,
}

#[repr(C)]
struct MSG {
    hwnd: isize,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: POINT,
}

#[repr(C)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct RAWINPUTDEVICE {
    usage_page: u16,
    usage: u16,
    flags: u32,
    hwnd_target: isize,
}

#[repr(C)]
struct RAWINPUTHEADER {
    dw_type: u32,
    dw_size: u32,
    h_device: isize,
    w_param: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct USAGE_AND_PAGE {
    usage: u16,
    usage_page: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KEYBDINPUT {
    w_vk: u16,
    w_scan: u16,
    dw_flags: u32,
    time: u32,
    dw_extra_info: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MOUSEINPUT {
    dx: i32,
    dy: i32,
    mouse_data: u32,
    dw_flags: u32,
    time: u32,
    dw_extra_info: usize,
}

#[repr(C)]
union INPUT_UNION {
    ki: KEYBDINPUT,
    mi: MOUSEINPUT,
}

#[repr(C)]
struct INPUT {
    kind: u32,
    u: INPUT_UNION,
}

#[repr(C)]
struct KBDLLHOOKSTRUCT {
    vk_code: u32,
    scan_code: u32,
    flags: u32,
    time: u32,
    dw_extra_info: usize,
}

#[repr(C)]
struct NOTIFYICONDATAW {
    cb_size: u32,
    hwnd: isize,
    u_id: u32,
    u_flags: u32,
    u_callback_message: u32,
    h_icon: isize,
    sz_tip: [u16; 128],
    dw_state: u32,
    dw_state_mask: u32,
    sz_info: [u16; 256],
    u_version: u32,
    sz_info_title: [u16; 64],
    dw_info_flags: u32,
    guid_item: [u8; 16],
    h_balloon_icon: isize,
}

#[repr(C)]
struct ICONINFO {
    f_icon: i32,
    x_hotspot: u32,
    y_hotspot: u32,
    hbm_mask: isize,
    hbm_color: isize,
}

#[repr(C)]
struct BITMAPINFOHEADER {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_x_pels_per_meter: i32,
    bi_y_pels_per_meter: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

#[link(name = "user32", kind = "raw-dylib")]
extern "system" {
    fn RegisterClassExW(wc: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(ex_style: u32, class: *const u16, name: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32,
                       parent: isize, menu: isize, instance: isize, param: *mut c_void) -> isize;
    fn DestroyWindow(hwnd: isize) -> i32;
    fn DefWindowProcW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn GetMessageW(msg: *mut MSG, hwnd: isize, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const MSG) -> i32;
    fn DispatchMessageW(msg: *const MSG) -> isize;
    fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn PostQuitMessage(code: i32);
    fn RegisterWindowMessageW(name: *const u16) -> u32;
    fn RegisterRawInputDevices(devices: *const RAWINPUTDEVICE, count: u32, size: u32) -> i32;
    fn GetRawInputData(raw: isize, command: u32, data: *mut c_void, size: *mut u32, header_size: u32) -> u32;
    fn GetRawInputDeviceInfoW(device: isize, command: u32, data: *mut c_void, size: *mut u32) -> u32;
    fn SendInput(count: u32, inputs: *const INPUT, size: i32) -> u32;
    fn SetWindowsHookExW(id: i32, proc_: Option<HookProc>, module: isize, thread: u32) -> isize;
    fn UnhookWindowsHookEx(hook: isize) -> i32;
    fn CallNextHookEx(hook: isize, code: i32, wparam: usize, lparam: isize) -> isize;
    fn SetTimer(hwnd: isize, id: usize, ms: u32, func: *const c_void) -> usize;
    fn KillTimer(hwnd: isize, id: usize) -> i32;
    fn SystemParametersInfoW(action: u32, param: u32, value: *mut c_void, win_ini: u32) -> i32;
    fn CreatePopupMenu() -> isize;
    fn AppendMenuW(menu: isize, flags: u32, id: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(menu: isize, flags: u32, x: i32, y: i32, reserved: i32, hwnd: isize, rect: *const RECT) -> i32;
    fn DestroyMenu(menu: isize) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn GetCursorPos(pt: *mut POINT) -> i32;
    fn CreateIconIndirect(info: *const ICONINFO) -> isize;
    fn DrawTextW(dc: isize, text: *const u16, len: i32, rect: *mut RECT, format: u32) -> i32;
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
    fn CreateMutexW(attrs: *const c_void, owner: i32, name: *const u16) -> isize;
    fn GetLastError() -> u32;
}

#[link(name = "gdi32", kind = "raw-dylib")]
extern "system" {
    fn CreateCompatibleDC(dc: isize) -> isize;
    fn DeleteDC(dc: isize) -> i32;
    fn CreateDIBSection(dc: isize, info: *const BITMAPINFOHEADER, usage: u32, bits: *mut *mut c_void, section: isize, offset: u32) -> isize;
    fn CreateBitmap(w: i32, h: i32, planes: u32, bpp: u32, bits: *const c_void) -> isize;
    fn SelectObject(dc: isize, obj: isize) -> isize;
    fn DeleteObject(obj: isize) -> i32;
    fn CreateFontW(h: i32, w: i32, esc: i32, orient: i32, weight: i32, italic: u32, underline: u32, strike: u32, charset: u32,
                   out_prec: u32, clip_prec: u32, quality: u32, pitch: u32, face: *const u16) -> isize;
    fn SetBkMode(dc: isize, mode: i32) -> i32;
    fn SetTextColor(dc: isize, color: u32) -> u32;
    fn GdiFlush() -> i32;
}

#[link(name = "shell32", kind = "raw-dylib")]
extern "system" {
    fn Shell_NotifyIconW(message: u32, data: *mut NOTIFYICONDATAW) -> i32;
}

#[link(name = "advapi32", kind = "raw-dylib")]
extern "system" {
    fn RegGetValueW(key: isize, sub_key: *const u16, value: *const u16, flags: u32, kind: *mut u32, data: *mut c_void,
                    size: *mut u32) -> i32;
    fn RegSetKeyValueW(key: isize, sub_key: *const u16, value: *const u16, kind: u32, data: *const c_void, size: u32) -> i32;
    fn RegDeleteKeyValueW(key: isize, sub_key: *const u16, value: *const u16) -> i32;
}

#[link(name = "hid", kind = "raw-dylib")]
extern "system" {
    fn HidP_GetUsagesEx(report_type: i32, link_collection: u16, list: *mut USAGE_AND_PAGE, length: *mut u32,
                        preparsed: *const c_void, report: *mut u8, report_length: u32) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn copy_wide(dst: &mut [u16], s: &str) {
    let src: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..src.len()].copy_from_slice(&src);
    dst[src.len()] = 0;
}

// ---- Remapping ----------------------------------------------------------------------------

const REPEAT_TIMER: usize = 1;
const CMD_PAUSE: i32 = 1;
const CMD_EXIT: i32 = 2;
const CMD_STARTUP: i32 = 3;

static ENABLED: AtomicBool = AtomicBool::new(true);
static ICON: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct Device {
    is_trio: bool,
    preparsed: Vec<u64>,
}

impl Device {
    unsafe fn query(h: isize) -> Device {
        let mut dev = Device { is_trio: false, preparsed: Vec::new() };
        let mut len = 0u32;
        GetRawInputDeviceInfoW(h, RIDI_DEVICENAME, null_mut(), &mut len);
        if len == 0 {
            return dev;
        }
        let mut name = vec![0u16; len as usize];
        if GetRawInputDeviceInfoW(h, RIDI_DEVICENAME, name.as_mut_ptr().cast(), &mut len) == u32::MAX {
            return dev;
        }
        dev.is_trio = String::from_utf16_lossy(&name).to_uppercase().contains(DEVICE_FILTER);
        if dev.is_trio {
            let mut size = 0u32;
            GetRawInputDeviceInfoW(h, RIDI_PREPARSEDDATA, null_mut(), &mut size);
            dev.preparsed = vec![0u64; (size as usize).div_ceil(8)];
            GetRawInputDeviceInfoW(h, RIDI_PREPARSEDDATA, dev.preparsed.as_mut_ptr().cast(), &mut size);
        }
        dev
    }
}

struct Keys {
    hwnd: isize,
    pressed: HashSet<u16>,
    repeat_scan: u16,
}

impl Keys {
    unsafe fn apply(&mut self, preparsed: *const c_void, report: &[u8]) {
        let mut list = [USAGE_AND_PAGE { usage: 0, usage_page: 0 }; 16];
        let mut len = list.len() as u32;
        let mut report = report.to_vec();
        if HidP_GetUsagesEx(0, 0, list.as_mut_ptr(), &mut len, preparsed, report.as_mut_ptr(), report.len() as u32)
            != HIDP_STATUS_SUCCESS
        {
            return;
        }
        let now: HashSet<u16> = list[..len as usize]
            .iter()
            .filter(|u| u.usage_page == 0x0C && scan_for(u.usage).is_some())
            .map(|u| u.usage)
            .collect();
        let released: Vec<u16> = self.pressed.difference(&now).copied().collect();
        for usage in released {
            self.key_up(usage);
        }
        for usage in now {
            if !self.pressed.contains(&usage) {
                self.key_down(usage);
            }
        }
    }

    unsafe fn key_down(&mut self, usage: u16) {
        let Some(scan) = scan_for(usage) else { return };
        self.pressed.insert(usage);
        self.repeat_scan = scan;
        send_key(scan, false);
        SetTimer(self.hwnd, REPEAT_TIMER, repeat_delay_ms(), null());
    }

    unsafe fn key_up(&mut self, usage: u16) {
        let Some(scan) = scan_for(usage) else { return };
        self.pressed.remove(&usage);
        if scan == self.repeat_scan {
            KillTimer(self.hwnd, REPEAT_TIMER);
            self.repeat_scan = 0;
        }
        send_key(scan, true);
    }

    unsafe fn release_all(&mut self) {
        let held: Vec<u16> = self.pressed.iter().copied().collect();
        for usage in held {
            self.key_up(usage);
        }
    }

    unsafe fn repeat(&mut self) {
        if self.repeat_scan == 0 {
            KillTimer(self.hwnd, REPEAT_TIMER);
            return;
        }
        SetTimer(self.hwnd, REPEAT_TIMER, repeat_rate_ms(), null());
        send_key(self.repeat_scan, false);
    }
}

struct State {
    devices: HashMap<isize, Device>,
    keys: Keys,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with_state(f: impl FnOnce(&mut State)) {
    STATE.with(|s| {
        if let Some(state) = s.borrow_mut().as_mut() {
            f(state)
        }
    });
}

unsafe fn repeat_delay_ms() -> u32 {
    let mut delay = 1u32;
    SystemParametersInfoW(SPI_GETKEYBOARDDELAY, 0, (&mut delay as *mut u32).cast(), 0);
    250 * (delay + 1)
}

unsafe fn repeat_rate_ms() -> u32 {
    let mut speed = 31u32;
    SystemParametersInfoW(SPI_GETKEYBOARDSPEED, 0, (&mut speed as *mut u32).cast(), 0);
    1000 / (3 + speed)
}

unsafe fn send_key(scan: u16, up: bool) {
    let flags = KEYEVENTF_SCANCODE | if up { KEYEVENTF_KEYUP } else { 0 };
    let input = INPUT {
        kind: INPUT_KEYBOARD,
        u: INPUT_UNION { ki: KEYBDINPUT { w_vk: 0, w_scan: scan, dw_flags: flags, time: 0, dw_extra_info: 0 } },
    };
    SendInput(1, &input, size_of::<INPUT>() as i32);
}

unsafe fn handle_input(raw: isize) {
    let header_size = size_of::<RAWINPUTHEADER>() as u32;
    let mut size = 0u32;
    GetRawInputData(raw, RID_INPUT, null_mut(), &mut size, header_size);
    if size == 0 {
        return;
    }
    let mut buf = vec![0u64; (size as usize).div_ceil(8)];
    if GetRawInputData(raw, RID_INPUT, buf.as_mut_ptr().cast(), &mut size, header_size) != size {
        return;
    }
    let base = buf.as_ptr() as *const u8;
    let header = &*(base as *const RAWINPUTHEADER);
    if header.dw_type != RIM_TYPEHID {
        return;
    }
    let hid = base.add(header_size as usize);
    let size_hid = *(hid as *const u32) as usize;
    let count = *(hid.add(4) as *const u32) as usize;
    if size_hid == 0 {
        return;
    }
    let reports = std::slice::from_raw_parts(hid.add(8), size_hid * count);

    with_state(|state| {
        let dev = state.devices.entry(header.h_device).or_insert_with(|| Device::query(header.h_device));
        if !dev.is_trio || dev.preparsed.is_empty() {
            return;
        }
        for report in reports.chunks(size_hid) {
            state.keys.apply(dev.preparsed.as_ptr().cast(), report);
        }
    });
}

// Without Fn, F6 sends AC Home, which Windows' HID service injects as the Browser Home key
// (so it carries the injected flag) and which opens a browser.
unsafe extern "system" fn keyboard_hook(code: i32, wparam: usize, lparam: isize) -> isize {
    if code >= 0 && ENABLED.load(Relaxed) && (*(lparam as *const KBDLLHOOKSTRUCT)).vk_code == VK_BROWSER_HOME {
        return 1;
    }
    CallNextHookEx(0, code, wparam, lparam)
}

// ---- Run at Windows startup ---------------------------------------------------------------

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Trio500FnFix";

fn startup_command() -> Option<String> {
    std::env::current_exe().ok().map(|path| format!("\"{}\"", path.display()))
}

// Only counts as enabled when the Run entry points at this exe, so a moved exe can re-register itself.
unsafe fn is_startup_enabled() -> bool {
    let Some(expected) = startup_command() else { return false };
    let mut data = [0u16; 1024];
    let mut size = (data.len() * 2) as u32;
    let status = RegGetValueW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), wide(RUN_VALUE).as_ptr(), RRF_RT_REG_SZ,
                              null_mut(), data.as_mut_ptr().cast(), &mut size);
    if status != 0 {
        return false;
    }
    let len = data.iter().position(|&c| c == 0).unwrap_or(data.len());
    String::from_utf16_lossy(&data[..len]).eq_ignore_ascii_case(&expected)
}

unsafe fn set_startup(enable: bool) {
    let key = wide(RUN_KEY);
    let value = wide(RUN_VALUE);
    if enable {
        let Some(command) = startup_command() else { return };
        let data = wide(&command);
        RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32);
    } else {
        RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr());
    }
}

// ---- Tray icon ----------------------------------------------------------------------------

fn tip_text() -> &'static str {
    if ENABLED.load(Relaxed) { "Trio 500 Fn Fix: On" } else { "Trio 500 Fn Fix: Paused" }
}

unsafe fn tray(hwnd: isize, message: u32, flags: u32, balloon: bool) {
    let mut nid: NOTIFYICONDATAW = zeroed();
    nid.cb_size = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hwnd = hwnd;
    nid.u_id = 1;
    nid.u_flags = flags;
    nid.u_callback_message = WM_TRAY;
    nid.h_icon = ICON.load(Relaxed);
    copy_wide(&mut nid.sz_tip, tip_text());
    if balloon {
        nid.u_flags |= NIF_INFO;
        nid.dw_info_flags = NIIF_INFO;
        copy_wide(&mut nid.sz_info_title, "Trio 500 Fn Fix");
        copy_wide(&mut nid.sz_info, "F1-F6 and F10-F12 now work without Fn.");
    }
    Shell_NotifyIconW(message, &mut nid);
}

unsafe fn make_icon() -> isize {
    const S: i32 = 32;
    let in_box = |x: i32, y: i32| (1..31).contains(&x) && (4..28).contains(&y);

    let mut bmi: BITMAPINFOHEADER = zeroed();
    bmi.bi_size = size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bi_width = S;
    bmi.bi_height = -S; // top-down
    bmi.bi_planes = 1;
    bmi.bi_bit_count = 32;
    let dc = CreateCompatibleDC(0);
    let mut bits: *mut c_void = null_mut();
    let color = CreateDIBSection(dc, &bmi, 0, &mut bits, 0, 0);
    let old_bitmap = SelectObject(dc, color);
    let px = std::slice::from_raw_parts_mut(bits as *mut u32, (S * S) as usize);
    for y in 0..S {
        for x in 0..S {
            px[(y * S + x) as usize] = if in_box(x, y) { 0x0014_28A0 } else { 0 };
        }
    }

    let font = CreateFontW(-14, 0, 0, 0, 700, 0, 0, 0, 0, 0, 0, 4, 0, wide("Segoe UI").as_ptr());
    let old_font = SelectObject(dc, font);
    SetBkMode(dc, 1);
    SetTextColor(dc, 0x00FF_FFFF);
    let mut rect = RECT { left: 0, top: 4, right: S, bottom: 28 };
    DrawTextW(dc, wide("F1").as_ptr(), -1, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    GdiFlush();
    // GDI text clears the alpha channel; make the whole box opaque again.
    for y in 0..S {
        for x in 0..S {
            if in_box(x, y) {
                px[(y * S + x) as usize] |= 0xFF00_0000;
            }
        }
    }
    SelectObject(dc, old_font);
    DeleteObject(font);
    SelectObject(dc, old_bitmap);

    let mask_bits = [0u8; (S * S / 8) as usize];
    let mask = CreateBitmap(S, S, 1, 1, mask_bits.as_ptr().cast());
    let info = ICONINFO { f_icon: 1, x_hotspot: 0, y_hotspot: 0, hbm_mask: mask, hbm_color: color };
    let icon = CreateIconIndirect(&info);
    DeleteObject(mask);
    DeleteObject(color);
    DeleteDC(dc);
    icon
}

unsafe fn set_enabled(hwnd: isize, enabled: bool) {
    ENABLED.store(enabled, Relaxed);
    if !enabled {
        with_state(|state| state.keys.release_all());
    }
    tray(hwnd, NIM_MODIFY, NIF_TIP, false);
}

unsafe fn show_menu(hwnd: isize) {
    let menu = CreatePopupMenu();
    let pause_flags = if ENABLED.load(Relaxed) { MF_STRING } else { MF_STRING | MF_CHECKED };
    let startup = is_startup_enabled();
    let startup_flags = if startup { MF_STRING | MF_CHECKED } else { MF_STRING };
    AppendMenuW(menu, pause_flags, CMD_PAUSE as usize, wide("Pause").as_ptr());
    AppendMenuW(menu, startup_flags, CMD_STARTUP as usize, wide("Run at Windows startup").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, CMD_EXIT as usize, wide("Exit").as_ptr());
    let mut pt = POINT::default();
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    match cmd {
        CMD_PAUSE => set_enabled(hwnd, !ENABLED.load(Relaxed)),
        CMD_STARTUP => set_startup(!startup),
        CMD_EXIT => {
            DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe extern "system" fn wnd_proc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        WM_INPUT => {
            if ENABLED.load(Relaxed) {
                handle_input(lparam);
            }
        }
        WM_TIMER if wparam == REPEAT_TIMER => {
            with_state(|state| state.keys.repeat());
            return 0;
        }
        WM_TRAY => {
            let event = lparam as u32 & 0xFFFF;
            if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                show_menu(hwnd);
            }
            return 0;
        }
        WM_DESTROY => {
            with_state(|state| state.keys.release_all());
            tray(hwnd, NIM_DELETE, 0, false);
            PostQuitMessage(0);
            return 0;
        }
        _ if msg != 0 && msg == TASKBAR_CREATED.load(Relaxed) => {
            // Explorer restarted; the tray icon has to be added again.
            tray(hwnd, NIM_ADD, NIF_MESSAGE | NIF_ICON | NIF_TIP, false);
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn main() {
    unsafe {
        // Named mutex, so only one instance runs at a time.
        CreateMutexW(null(), 1, wide("Trio500FnFix").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        let instance = GetModuleHandleW(null());
        let class_name = wide("Trio500FnFix");
        let mut wc: WNDCLASSEXW = zeroed();
        wc.cb_size = size_of::<WNDCLASSEXW>() as u32;
        wc.lpfn_wnd_proc = Some(wnd_proc);
        wc.h_instance = instance;
        wc.lpsz_class_name = class_name.as_ptr();
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(0, class_name.as_ptr(), class_name.as_ptr(), 0, 0, 0, 0, 0, 0, 0, instance, null_mut());
        TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Relaxed);

        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                devices: HashMap::new(),
                keys: Keys { hwnd, pressed: HashSet::new(), repeat_scan: 0 },
            })
        });

        let rid = RAWINPUTDEVICE { usage_page: 0x0C, usage: 0x01, flags: RIDEV_INPUTSINK, hwnd_target: hwnd };
        RegisterRawInputDevices(&rid, 1, size_of::<RAWINPUTDEVICE>() as u32);
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), instance, 0);

        ICON.store(make_icon(), Relaxed);
        tray(hwnd, NIM_ADD, NIF_MESSAGE | NIF_ICON | NIF_TIP, true);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        UnhookWindowsHookEx(hook);
    }
}
