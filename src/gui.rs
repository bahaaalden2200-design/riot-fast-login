use crate::riot::{ActiveAccount, RiotManager};
use crate::storage::StorageManager;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WINDOW_CLASS_NAME: &str = "RiotClientNativeWindow";
pub const WM_TRAYICON: u32 = WM_USER + 100;
pub const WM_APP_ASYNC_DONE: u32 = WM_USER + 101;
const WM_MOUSELEAVE_MSG: u32 = 0x02A3;

const ID_TRAY_OPEN: usize = 2001;
const ID_TRAY_NEXT: usize = 2002;
const ID_TRAY_KILL: usize = 2003;
const ID_TRAY_EXIT: usize = 2004;

const ID_CTX_SWITCH_BASE: usize = 3000;
const ID_CTX_DELETE_BASE: usize = 4000;

const TIMER_AUTODETECT: usize = 1001;

/// Helper to construct Win32 COLORREF (0x00BBGGRR) from standard RGB
const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

// =========================================================================
// GOOGLE AI PRO — DEEP OLED BLACK & CRIMSON RED PALETTE (COMPACT FLYOUT)
// =========================================================================
const COLOR_OLED_BG: u32            = rgb(5, 5, 8);       // #050508 Deep OLED Flyout Background
const COLOR_OUTER_BORDER: u32       = rgb(46, 18, 26);    // #2E121A Subtle Crimson-Obsidian Frame
const COLOR_DIVIDER: u32            = rgb(24, 18, 24);    // #181218 Hairline Divider

// Card Surfaces
const COLOR_HERO_BG: u32            = rgb(11, 10, 15);    // #0B0A0F Upper Active Box Surface
const COLOR_HERO_BORDER: u32        = rgb(32, 22, 28);    // #20161C Upper Active Box Border
const COLOR_CARD_BG: u32            = rgb(11, 10, 15);    // #0B0A0F Compact Stream Card
const COLOR_CARD_HOVER: u32         = rgb(22, 12, 17);    // #160C11 Hover Stream Card
const COLOR_CARD_ACTIVE: u32        = rgb(20, 9, 14);     // #14090E Active Stream Card
const COLOR_CARD_BORDER: u32        = rgb(26, 23, 32);    // #1A1720 Idle Card Border
const COLOR_CARD_BORDER_HOVER: u32  = rgb(150, 24, 48);   // #961830 Hover Card Border
const COLOR_CARD_BORDER_ACTIVE: u32 = rgb(255, 38, 72);   // #FF2648 Active Card Bright Red Border

// Google AI Pro Crimson & Status Accents
const COLOR_PRO_RED: u32            = rgb(255, 36, 70);   // #FF2446 Electric Google AI Pro Red
const COLOR_RIOT_RED: u32           = rgb(225, 12, 48);   // #E10C30 Deep Riot Crimson
const COLOR_RED_PILL_BG: u32        = rgb(36, 8, 16);     // #240810 Crimson Pill Fill
const COLOR_RED_PILL_HOVER: u32     = rgb(56, 12, 24);    // #380C18 Crimson Pill Hover
const COLOR_RED_PILL_BORDER: u32    = rgb(155, 22, 48);   // #9B1630 Crimson Pill Border
const COLOR_RED_PILL_TEXT: u32      = rgb(255, 105, 128); // #FF6980 Crimson Pill Text

const COLOR_BTN_DARK_BG: u32        = rgb(17, 15, 22);    // #110F16 Dark Button Fill
const COLOR_BTN_DARK_HOVER: u32     = rgb(28, 22, 32);    // #1C1620 Dark Button Hover
const COLOR_BTN_DARK_BORDER: u32    = rgb(38, 32, 46);    // #26202E Dark Button Border

const COLOR_ACTIVE_BADGE_BG: u32    = rgb(12, 36, 22);    // #0C2416 Active Green Pill Fill
const COLOR_ACTIVE_BADGE_BD: u32    = rgb(26, 82, 48);    // #1A5230 Active Green Pill Border
const COLOR_GOOGLE_GREEN: u32       = rgb(52, 211, 105);  // #34D369 Vibrant Emerald Green

// Typography Colors
const COLOR_TEXT_WHITE: u32         = rgb(245, 246, 250); // #F5F6FA Primary Crisp White
const COLOR_TEXT_SECONDARY: u32     = rgb(150, 156, 168); // #969CA8 Secondary Gray
const COLOR_TEXT_MUTED: u32         = rgb(98, 102, 116);  // #626674 Muted Dim Gray

// Compact Flyout Dimensions
const WIN_WIDTH: i32       = 392;
const WIN_HEIGHT: i32      = 522;
const PAD_X: i32           = 14;
const TOPBAR_H: i32        = 44;
const HERO_TOP_Y: i32      = 48;
const HERO_H: i32          = 104;
const LIST_START_Y: i32    = 162;
const FOOTER_START_Y: i32  = 454;
const CARD_H: i32          = 46;
const CARD_SPACING: i32    = 7;

// Win32 Constants
const IMAGE_ICON_TYPE: u32      = 1;
const LR_LOADFROMFILE_FLAG: u32 = 0x00000010;

// Authentic Riot Games Official Typography
const FONT_BEAUFORT_BOLD: &[u8]   = include_bytes!("../assets/fonts/BeaufortforLOL-Bold.ttf");
const FONT_BEAUFORT_MEDIUM: &[u8] = include_bytes!("../assets/fonts/BeaufortforLOL-Medium.ttf");

// Unicode Vector Glyphs (Segoe Fluent Icons / Segoe MDL2 Assets)
const GLYPH_REFRESH: &str  = "\u{E72C}";
const GLYPH_PIN: &str      = "\u{E718}";
const GLYPH_CLOSE: &str    = "\u{E8BB}";
const GLYPH_POWER: &str    = "\u{E7E8}";
const GLYPH_BOLT: &str     = "\u{E945}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitArea {
    TitleBar,
    TabRiot,
    TabVal,
    TabLol,
    BtnSync,
    BtnPin,
    BtnClose,
    BtnSwitchMenu,
    BtnNextAccount,
    Account(usize),
    ToggleAutoLaunch,
    BtnSaveCurrent,
    AddAccount,
    BtnKillRiot,
}

struct GuiState {
    riot: RiotManager,
    storage: StorageManager,
    active_account: Option<ActiveAccount>,
    hovered: Option<HitArea>,
    feedback_text: String,
    feedback_is_error: bool,
    is_busy: bool,
    is_pinned: bool,
    auto_launch: bool,
    tracking_mouse: bool,
    scroll_offset: i32,
}

unsafe impl Send for GuiState {}
unsafe impl Sync for GuiState {}

static GUI_STATE: OnceLock<Arc<Mutex<GuiState>>> = OnceLock::new();

pub fn to_wstring(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Computes bottom-right coordinates docked directly above the Windows Taskbar
pub unsafe fn get_taskbar_dock_coords(width: i32, height: i32) -> (i32, i32) {
    let mut work_rc: RECT = std::mem::zeroed();
    if SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        &mut work_rc as *mut _ as *mut _,
        0,
    ) != 0
    {
        let x = work_rc.right - width - 12;
        let y = work_rc.bottom - height - 12;
        (x.max(10), y.max(10))
    } else {
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        (screen_w - width - 16, screen_h - height - 56)
    }
}

pub unsafe fn load_official_riot_icon_sized(size: i32) -> HICON {
    let paths = [
        r"C:\Riot Games\Riot Client\Resources\icon.ico",
        r"C:\Riot Games\Riot Client\RiotClientElectron\resources\assets\icon.ico",
    ];

    for path_str in paths {
        let path_w = to_wstring(path_str);
        let hicon = LoadImageW(
            0 as HINSTANCE,
            path_w.as_ptr(),
            IMAGE_ICON_TYPE,
            size,
            size,
            LR_LOADFROMFILE_FLAG,
        );
        if hicon != 0 as HANDLE {
            return hicon as HICON;
        }
    }

    LoadIconW(0 as HINSTANCE, IDI_APPLICATION)
}

pub unsafe fn load_app_window_icon_sized(size: i32) -> HICON {
    let hinst = GetModuleHandleW(null_mut()) as HINSTANCE;
    for &id in &[0usize, 1usize] {
        let embedded = LoadImageW(
            hinst,
            id as *const u16,
            IMAGE_ICON_TYPE,
            size,
            size,
            0,
        );
        if embedded != 0 as HANDLE {
            return embedded as HICON;
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let local_asset = parent.join("assets").join("app.ico");
            if local_asset.exists() {
                let path_w = to_wstring(local_asset.to_str().unwrap_or_default());
                let hicon = LoadImageW(
                    0 as HINSTANCE,
                    path_w.as_ptr(),
                    IMAGE_ICON_TYPE,
                    size,
                    size,
                    LR_LOADFROMFILE_FLAG,
                );
                if hicon != 0 as HANDLE {
                    return hicon as HICON;
                }
            }
        }
    }

    load_official_riot_icon_sized(size)
}

pub unsafe fn hide_to_tray(hwnd: HWND) {
    let mut ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
    ex_style &= !(WS_EX_APPWINDOW as i32);
    ex_style |= WS_EX_TOOLWINDOW as i32;
    SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style);
    ShowWindow(hwnd, SW_HIDE);
}

pub unsafe fn restore_from_tray(hwnd: HWND) {
    let (dock_x, dock_y) = get_taskbar_dock_coords(WIN_WIDTH, WIN_HEIGHT);
    let is_pinned = GUI_STATE
        .get()
        .and_then(|s| s.lock().ok().map(|st| st.is_pinned))
        .unwrap_or(true);

    SetWindowPos(
        hwnd,
        if is_pinned {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        },
        dock_x,
        dock_y,
        WIN_WIDTH,
        WIN_HEIGHT,
        SWP_SHOWWINDOW,
    );
    ShowWindow(hwnd, SW_RESTORE);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
}

fn format_compact_age(last_used_secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if last_used_secs == 0 || now <= last_used_secs {
        return "just now".to_string();
    }
    let diff = now - last_used_secs;
    if diff < 3600 {
        "just now".to_string()
    } else if diff < 86400 {
        format!("{}h", diff / 3600)
    } else {
        let d = diff / 86400;
        let h = (diff % 86400) / 3600;
        if h > 0 && d < 14 {
            format!("{}d {}h", d, h)
        } else {
            format!("{}d", d)
        }
    }
}

fn check_and_autosave(state: &mut GuiState) -> bool {
    if state.is_busy {
        return false;
    }

    let current_active = state.riot.get_active_account();
    let had_change = match (&state.active_account, &current_active) {
        (Some(old), Some(new)) => {
            old.game_name != new.game_name
                || old.tag_line != new.tag_line
                || old.last_token_time != new.last_token_time
        }
        (None, Some(_)) => true,
        (Some(_), None) => true,
        (None, None) => false,
    };

    state.active_account = current_active.clone();

    if let Some(ref act) = current_active {
        if !act.game_name.is_empty() {
            let conf = state.storage.load_config();
            let already_saved = conf.accounts.iter().any(|a| {
                a.game_name.eq_ignore_ascii_case(&act.game_name)
                    && a.tag_line.eq_ignore_ascii_case(&act.tag_line)
            });

            if let Ok(p) = state
                .storage
                .save_active_account(act, None, &state.riot.settings_yaml_path)
            {
                if !already_saved {
                    state.feedback_text =
                        format!("✦ Linked: {} #{}", p.game_name, p.tag_line);
                    state.feedback_is_error = false;
                    return true;
                }
            }
        }
    }

    had_change
}

pub fn run_gui() {
    unsafe {
        let mut pc_fonts1: u32 = 0;
        AddFontMemResourceEx(
            FONT_BEAUFORT_BOLD.as_ptr() as *const _,
            FONT_BEAUFORT_BOLD.len() as u32,
            null_mut(),
            &mut pc_fonts1,
        );
        let mut pc_fonts2: u32 = 0;
        AddFontMemResourceEx(
            FONT_BEAUFORT_MEDIUM.as_ptr() as *const _,
            FONT_BEAUFORT_MEDIUM.len() as u32,
            null_mut(),
            &mut pc_fonts2,
        );

        let class_name = to_wstring(WINDOW_CLASS_NAME);
        let hinstance = GetModuleHandleW(null_mut()) as HINSTANCE;
        let app_icon_big = load_app_window_icon_sized(48);
        let app_icon_small = load_app_window_icon_sized(16);

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DROPSHADOW,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: app_icon_big,
            hCursor: LoadCursorW(0 as HINSTANCE, IDC_ARROW),
            hbrBackground: 0 as HBRUSH,
            lpszMenuName: null_mut(),
            lpszClassName: class_name.as_ptr(),
        };

        RegisterClassW(&wc);

        let (start_x, start_y) = get_taskbar_dock_coords(WIN_WIDTH, WIN_HEIGHT);

        let window_title = to_wstring("Riot Fast Login");
        // Compact Taskbar Tray Flyout Window (WS_EX_TOOLWINDOW | WS_EX_TOPMOST)
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            start_x,
            start_y,
            WIN_WIDTH,
            WIN_HEIGHT,
            0 as HWND,
            0 as HMENU,
            hinstance,
            null_mut(),
        );

        if hwnd == 0 as HWND {
            return;
        }

        SendMessageW(hwnd, WM_SETICON, ICON_BIG as WPARAM, app_icon_big as LPARAM);
        SendMessageW(
            hwnd,
            WM_SETICON,
            ICON_SMALL as WPARAM,
            app_icon_small as LPARAM,
        );

        // Add System Tray Icon
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = app_icon_small;
        let tip = to_wstring("Riot Fast Login PRO");
        let tip_len = tip.len().min(nid.szTip.len() - 1);
        nid.szTip[..tip_len].copy_from_slice(&tip[..tip_len]);
        Shell_NotifyIconW(NIM_ADD, &nid);

        // Enable Windows 11 Dark Mode & Smooth Rounded Flyout Corners
        let dark_mode: i32 = 1;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &dark_mode as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );
        let corner_pref: u32 = 2; // DWMWCP_ROUND
        DwmSetWindowAttribute(
            hwnd,
            33,
            &corner_pref as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );
        let border_col: u32 = COLOR_OUTER_BORDER;
        DwmSetWindowAttribute(
            hwnd,
            34,
            &border_col as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        let riot = RiotManager::new();
        let storage = StorageManager::new();

        let mut state = GuiState {
            riot,
            storage,
            active_account: None,
            hovered: None,
            feedback_text: String::new(),
            feedback_is_error: false,
            is_busy: false,
            is_pinned: true,
            auto_launch: true,
            tracking_mouse: false,
            scroll_offset: 0,
        };

        check_and_autosave(&mut state);

        let _ = GUI_STATE.set(Arc::new(Mutex::new(state)));

        SetTimer(hwnd, TIMER_AUTODETECT, 2000, None);

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetForegroundWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe fn trigger_account_switch(hwnd: HWND, idx: usize) {
    let (target_id, target_name, auto_launch, riot, storage) = {
        if let Some(state_arc) = GUI_STATE.get() {
            if let Ok(mut state) = state_arc.lock() {
                if state.is_busy {
                    return;
                }
                let conf = state.storage.load_config();
                if idx < conf.accounts.len() {
                    let id = conf.accounts[idx].id.clone();
                    let name = format!(
                        "{} #{}",
                        conf.accounts[idx].game_name, conf.accounts[idx].tag_line
                    );
                    state.is_busy = true;
                    state.feedback_text = format!("⚡ Switching to {}...", name);
                    state.feedback_is_error = false;
                    (
                        id,
                        name,
                        state.auto_launch,
                        state.riot.clone(),
                        state.storage.clone(),
                    )
                } else {
                    return;
                }
            } else {
                return;
            }
        } else {
            return;
        }
    };

    InvalidateRect(hwnd, null_mut(), 0);
    UpdateWindow(hwnd);

    let hwnd_val = hwnd as isize;
    std::thread::spawn(move || {
        storage.backup_current_active_session(&riot);
        RiotManager::kill_riot_processes();
        storage.backup_current_active_session(&riot);

        let res = storage.apply_profile(&target_id, &riot.settings_yaml_path);
        match res {
            Ok(_) => {
                if auto_launch {
                    let _ = riot.launch("riot_client");
                }
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        state.is_busy = false;
                        state.active_account = state.riot.get_active_account();
                        state.feedback_text = if auto_launch {
                            format!("✦ Active & Launched: {}", target_name)
                        } else {
                            format!("✦ Switched session: {}", target_name)
                        };
                        state.feedback_is_error = false;
                    }
                }
                PostMessageW(hwnd_val as HWND, WM_APP_ASYNC_DONE, 0, 0);

                if auto_launch {
                    for delay_secs in [4u64, 4u64] {
                        std::thread::sleep(std::time::Duration::from_secs(delay_secs));
                        storage.backup_current_active_session(&riot);
                        PostMessageW(hwnd_val as HWND, WM_APP_ASYNC_DONE, 0, 0);
                    }
                }
            }
            Err(e) => {
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        state.is_busy = false;
                        state.feedback_text = e;
                        state.feedback_is_error = true;
                    }
                }
                PostMessageW(hwnd_val as HWND, WM_APP_ASYNC_DONE, 0, 0);
            }
        }
    });
}

unsafe fn trigger_next_account(hwnd: HWND) {
    let next_idx = {
        if let Some(state_arc) = GUI_STATE.get() {
            if let Ok(state) = state_arc.lock() {
                let conf = state.storage.load_config();
                if conf.accounts.is_empty() {
                    None
                } else if let Some(ref act) = state.active_account {
                    let curr_pos = conf.accounts.iter().position(|a| {
                        a.game_name.eq_ignore_ascii_case(&act.game_name)
                            && a.tag_line.eq_ignore_ascii_case(&act.tag_line)
                    });
                    Some(match curr_pos {
                        Some(p) => (p + 1) % conf.accounts.len(),
                        None => 0,
                    })
                } else {
                    Some(0)
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some(idx) = next_idx {
        trigger_account_switch(hwnd, idx);
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_APP_ASYNC_DONE => {
            InvalidateRect(hwnd, null_mut(), 0);
            UpdateWindow(hwnd);
            0
        }
        WM_ACTIVATE => {
            let active = (wparam & 0xFFFF) as u32;
            if active == WA_INACTIVE {
                let is_pinned = GUI_STATE
                    .get()
                    .and_then(|s| s.lock().ok().map(|st| st.is_pinned))
                    .unwrap_or(true);
                if !is_pinned {
                    hide_to_tray(hwnd);
                }
            }
            0
        }
        WM_TIMER => {
            if wparam == TIMER_AUTODETECT {
                let mut changed = false;
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        changed = check_and_autosave(&mut state);
                    }
                }
                if changed {
                    InvalidateRect(hwnd, null_mut(), 0);
                }
            }
            0
        }
        WM_TRAYICON => {
            let event = (lparam & 0xFFFF) as u32;
            match event {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    if IsWindowVisible(hwnd) == 0 {
                        restore_from_tray(hwnd);
                    } else {
                        hide_to_tray(hwnd);
                    }
                }
                WM_RBUTTONUP => {
                    let mut pt: POINT = std::mem::zeroed();
                    GetCursorPos(&mut pt);
                    SetForegroundWindow(hwnd);

                    let hmenu = CreatePopupMenu();
                    let open_str = to_wstring("Open / Hide Flyout");
                    let next_str = to_wstring("Switch to Next Account");
                    let kill_str = to_wstring("Close Riot Client");
                    let exit_str = to_wstring("Exit");
                    AppendMenuW(hmenu, MF_STRING, ID_TRAY_OPEN, open_str.as_ptr());
                    AppendMenuW(hmenu, MF_STRING, ID_TRAY_NEXT, next_str.as_ptr());
                    AppendMenuW(hmenu, MF_STRING, ID_TRAY_KILL, kill_str.as_ptr());
                    AppendMenuW(hmenu, MF_SEPARATOR, 0, null_mut());
                    AppendMenuW(hmenu, MF_STRING, ID_TRAY_EXIT, exit_str.as_ptr());

                    TrackPopupMenu(
                        hmenu,
                        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                        pt.x,
                        pt.y,
                        0,
                        hwnd,
                        null_mut(),
                    );
                    PostMessageW(hwnd, WM_NULL, 0, 0);
                    DestroyMenu(hmenu);
                }
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as usize;
            if id == ID_TRAY_OPEN {
                if IsWindowVisible(hwnd) == 0 {
                    restore_from_tray(hwnd);
                } else {
                    hide_to_tray(hwnd);
                }
            } else if id == ID_TRAY_NEXT {
                trigger_next_account(hwnd);
            } else if id == ID_TRAY_KILL {
                std::thread::spawn(|| {
                    RiotManager::kill_riot_processes();
                });
            } else if id == ID_TRAY_EXIT {
                DestroyWindow(hwnd);
            } else if id >= ID_CTX_SWITCH_BASE && id < ID_CTX_SWITCH_BASE + 500 {
                let idx = id - ID_CTX_SWITCH_BASE;
                trigger_account_switch(hwnd, idx);
            } else if id >= ID_CTX_DELETE_BASE && id < ID_CTX_DELETE_BASE + 500 {
                let idx = id - ID_CTX_DELETE_BASE;
                let target_id = {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(state) = state_arc.lock() {
                            let conf = state.storage.load_config();
                            conf.accounts.get(idx).map(|a| a.id.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                if let Some(tid) = target_id {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(mut state) = state_arc.lock() {
                            let _ = state.storage.delete_profile(&tid);
                            state.feedback_text = "Account deleted.".to_string();
                            state.feedback_is_error = false;
                        }
                    }
                    InvalidateRect(hwnd, null_mut(), 0);
                }
            }
            0
        }
        WM_SYSCOMMAND => {
            if (wparam & 0xFFF0) == SC_MINIMIZE as usize {
                hide_to_tray(hwnd);
                0
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) & 0xFFFF) as i16 as i32;
            let mut needs_redraw = false;
            if let Some(state_arc) = GUI_STATE.get() {
                if let Ok(mut state) = state_arc.lock() {
                    let acc_count = state.storage.load_config().accounts.len();
                    let total_content_h =
                        (acc_count as i32) * (CARD_H + CARD_SPACING) - CARD_SPACING;
                    let visible_h = FOOTER_START_Y - LIST_START_Y - 8;
                    let max_scroll = (total_content_h - visible_h).max(0);

                    let step = (delta / 120) * 36;
                    let new_scroll = (state.scroll_offset - step).clamp(0, max_scroll);
                    if state.scroll_offset != new_scroll {
                        state.scroll_offset = new_scroll;
                        needs_redraw = true;
                    }
                }
            }
            if needs_redraw {
                let mut pt: POINT = std::mem::zeroed();
                GetCursorPos(&mut pt);
                ScreenToClient(hwnd, &mut pt);
                handle_mouse_move(hwnd, pt.x, pt.y);
                InvalidateRect(hwnd, null_mut(), 0);
            }
            0
        }
        WM_MOUSELEAVE_MSG => {
            let mut needs_redraw = false;
            if let Some(state_arc) = GUI_STATE.get() {
                if let Ok(mut state) = state_arc.lock() {
                    state.tracking_mouse = false;
                    if state.hovered.is_some() {
                        state.hovered = None;
                        needs_redraw = true;
                    }
                }
            }
            if needs_redraw {
                InvalidateRect(hwnd, null_mut(), 0);
            }
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            render_riot_client(hwnd, hdc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_MOUSEMOVE => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            handle_mouse_move(hwnd, x, y);
            0
        }
        WM_LBUTTONDOWN => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            handle_mouse_down(hwnd, x, y);
            0
        }
        WM_RBUTTONUP => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            handle_right_click(hwnd, x, y);
            0
        }
        WM_DESTROY => {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &nid);

            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn create_font(name: &str, size: i32, bold: bool) -> HFONT {
    let font_name = to_wstring(name);
    CreateFontW(
        size,
        0,
        0,
        0,
        if bold { FW_BOLD as i32 } else { FW_NORMAL as i32 },
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        font_name.as_ptr(),
    )
}

unsafe fn create_riot_font(size: i32, bold: bool) -> HFONT {
    let font = create_font("Beaufort for LOL", size, bold);
    if font != 0 as HFONT {
        font
    } else {
        create_font("Segoe UI", size, bold)
    }
}

unsafe fn create_riot_sub_font(size: i32, bold: bool) -> HFONT {
    let font = create_font("Beaufort for LOL Medium", size, bold);
    if font != 0 as HFONT {
        font
    } else {
        create_font("Segoe UI", size, bold)
    }
}

unsafe fn create_vector_icon_font(size: i32) -> HFONT {
    let font = create_font("Segoe Fluent Icons", size, false);
    if font != 0 as HFONT {
        font
    } else {
        create_font("Segoe MDL2 Assets", size, false)
    }
}

unsafe fn draw_round_rect(
    hdc: HDC,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    radius: i32,
    fill_color: u32,
    border_color: u32,
    border_width: i32,
) {
    let brush = CreateSolidBrush(fill_color);
    let pen = if border_width > 0 {
        CreatePen(PS_SOLID as i32, border_width, border_color)
    } else {
        CreatePen(PS_NULL as i32, 0, 0)
    };

    let old_brush = SelectObject(hdc, brush as *mut _);
    let old_pen = SelectObject(hdc, pen as *mut _);

    RoundRect(hdc, left, top, right, bottom, radius, radius);

    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(brush as *mut _);
    DeleteObject(pen as *mut _);
}

unsafe fn fill_rect_color(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32, color: u32) {
    let brush = CreateSolidBrush(color);
    let rc = RECT {
        left,
        top,
        right,
        bottom,
    };
    FillRect(hdc, &rc, brush as *mut _);
    DeleteObject(brush as *mut _);
}

unsafe fn draw_circle(hdc: HDC, cx: i32, cy: i32, radius: i32, fill_color: u32) {
    let brush = CreateSolidBrush(fill_color);
    let pen = CreatePen(PS_NULL as i32, 0, 0);
    let old_b = SelectObject(hdc, brush as *mut _);
    let old_p = SelectObject(hdc, pen as *mut _);
    Ellipse(hdc, cx - radius, cy - radius, cx + radius + 1, cy + radius + 1);
    SelectObject(hdc, old_b);
    SelectObject(hdc, old_p);
    DeleteObject(brush as *mut _);
    DeleteObject(pen as *mut _);
}

// =========================================================================
// MAIN RENDER LOOP — COMPACT TASKBAR TRAY FLYOUT (GOOGLE AI PRO OLED & RED)
// =========================================================================

unsafe fn render_riot_client(hwnd: HWND, hdc: HDC) {
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    let mem_dc = CreateCompatibleDC(hdc);
    let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
    let old_bmp = SelectObject(mem_dc, mem_bmp as *mut _);

    // 1. Deep OLED Black Flyout Canvas + Subtle Outer Crimson Border
    fill_rect_color(mem_dc, 0, 0, width, height, COLOR_OLED_BG);
    let border_pen = CreatePen(PS_SOLID as i32, 1, COLOR_OUTER_BORDER);
    let old_p = SelectObject(mem_dc, border_pen as *mut _);
    let old_b = SelectObject(mem_dc, GetStockObject(NULL_BRUSH as i32));
    Rectangle(mem_dc, 0, 0, width, height);
    SelectObject(mem_dc, old_b);
    SelectObject(mem_dc, old_p);
    DeleteObject(border_pen as *mut _);

    // Typography
    let font_pill = create_riot_sub_font(12, true);
    let font_hero_name = create_riot_font(16, true);
    let font_badge = create_riot_sub_font(11, true);
    let font_btn = create_riot_sub_font(13, true);
    let font_card_name = create_riot_font(15, true);
    let font_card_meta = create_riot_sub_font(12, true);
    let font_server = create_riot_font(14, true);
    let font_footer_main = create_riot_sub_font(12, true);
    let font_footer_sub = create_riot_sub_font(10, false);
    let font_sparkle = create_font("Segoe UI Symbol", 14, true);
    let font_icon = create_vector_icon_font(12);

    SetBkMode(mem_dc, TRANSPARENT as i32);

    let (active_acc, config, hov, feedback, is_err, is_pinned, auto_launch, scroll_offset) = {
        if let Some(state_arc) = GUI_STATE.get() {
            if let Ok(state) = state_arc.lock() {
                let conf = state.storage.load_config();
                (
                    state.active_account.clone(),
                    conf,
                    state.hovered,
                    state.feedback_text.clone(),
                    state.feedback_is_error,
                    state.is_pinned,
                    state.auto_launch,
                    state.scroll_offset,
                )
            } else {
                return;
            }
        } else {
            return;
        }
    };

    // =========================================================================
    // 1. TOP NAVIGATION PILLS & HEADER TOOLS (y: 10..40)
    // =========================================================================
    let pill_y = 10;
    let pill_h = 28;

    // Pill 1: Active "✦ Riot ●"
    let riot_x = PAD_X;
    let riot_w = 86;
    let is_riot_hov = hov == Some(HitArea::TabRiot);
    draw_round_rect(
        mem_dc,
        riot_x,
        pill_y,
        riot_x + riot_w,
        pill_y + pill_h,
        14,
        if is_riot_hov {
            COLOR_RED_PILL_HOVER
        } else {
            COLOR_RED_PILL_BG
        },
        if is_riot_hov {
            COLOR_PRO_RED
        } else {
            COLOR_RED_PILL_BORDER
        },
        1,
    );
    SelectObject(mem_dc, font_pill as *mut _);
    SetTextColor(mem_dc, COLOR_RED_PILL_TEXT);
    let riot_txt = to_wstring("✦  Riot");
    let mut riot_rc = RECT {
        left: riot_x + 12,
        top: pill_y,
        right: riot_x + riot_w - 20,
        bottom: pill_y + pill_h,
    };
    DrawTextW(
        mem_dc,
        riot_txt.as_ptr(),
        -1,
        &mut riot_rc,
        (DT_LEFT | DT_VCENTER | DT_SINGLELINE) as u32,
    );
    draw_circle(mem_dc, riot_x + riot_w - 14, pill_y + 14, 3, COLOR_PRO_RED);

    // Pill 2: "</> VAL"
    let val_x = riot_x + riot_w + 6;
    let val_w = 66;
    let is_val_hov = hov == Some(HitArea::TabVal);
    if is_val_hov {
        draw_round_rect(
            mem_dc,
            val_x,
            pill_y,
            val_x + val_w,
            pill_y + pill_h,
            14,
            COLOR_CARD_HOVER,
            COLOR_CARD_BORDER_HOVER,
            1,
        );
    }
    SelectObject(mem_dc, font_pill as *mut _);
    SetTextColor(
        mem_dc,
        if is_val_hov {
            COLOR_TEXT_WHITE
        } else {
            COLOR_TEXT_SECONDARY
        },
    );
    let val_txt = to_wstring("</> VAL");
    let mut val_rc = RECT {
        left: val_x,
        top: pill_y,
        right: val_x + val_w,
        bottom: pill_y + pill_h,
    };
    DrawTextW(
        mem_dc,
        val_txt.as_ptr(),
        -1,
        &mut val_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // Pill 3: ">_ LoL"
    let lol_x = val_x + val_w + 4;
    let lol_w = 64;
    let is_lol_hov = hov == Some(HitArea::TabLol);
    if is_lol_hov {
        draw_round_rect(
            mem_dc,
            lol_x,
            pill_y,
            lol_x + lol_w,
            pill_y + pill_h,
            14,
            COLOR_CARD_HOVER,
            COLOR_CARD_BORDER_HOVER,
            1,
        );
    }
    SetTextColor(
        mem_dc,
        if is_lol_hov {
            COLOR_TEXT_WHITE
        } else {
            COLOR_TEXT_SECONDARY
        },
    );
    let lol_txt = to_wstring(">_ LoL");
    let mut lol_rc = RECT {
        left: lol_x,
        top: pill_y,
        right: lol_x + lol_w,
        bottom: pill_y + pill_h,
    };
    DrawTextW(
        mem_dc,
        lol_txt.as_ptr(),
        -1,
        &mut lol_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // Right Header Tool Icons: [ ↻ ] [ 📌 ] [ ✕ ]
    let tool_sz = 28;
    let close_x = width - PAD_X - tool_sz;
    let pin_x = close_x - tool_sz - 4;
    let sync_x = pin_x - tool_sz - 4;

    // Sync Button
    let is_sync_hov = hov == Some(HitArea::BtnSync);
    if is_sync_hov {
        draw_round_rect(
            mem_dc,
            sync_x,
            pill_y,
            sync_x + tool_sz,
            pill_y + tool_sz,
            8,
            COLOR_BTN_DARK_HOVER,
            COLOR_CARD_BORDER_HOVER,
            1,
        );
    }
    SelectObject(mem_dc, font_icon as *mut _);
    SetTextColor(
        mem_dc,
        if is_sync_hov {
            COLOR_TEXT_WHITE
        } else {
            COLOR_TEXT_SECONDARY
        },
    );
    let sync_w = to_wstring(GLYPH_REFRESH);
    let mut sync_rc = RECT {
        left: sync_x,
        top: pill_y,
        right: sync_x + tool_sz,
        bottom: pill_y + tool_sz,
    };
    DrawTextW(
        mem_dc,
        sync_w.as_ptr(),
        -1,
        &mut sync_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // Pin Button
    let is_pin_hov = hov == Some(HitArea::BtnPin);
    if is_pin_hov || is_pinned {
        draw_round_rect(
            mem_dc,
            pin_x,
            pill_y,
            pin_x + tool_sz,
            pill_y + tool_sz,
            8,
            if is_pinned {
                COLOR_RED_PILL_BG
            } else {
                COLOR_BTN_DARK_HOVER
            },
            if is_pinned {
                COLOR_RED_PILL_BORDER
            } else {
                COLOR_BTN_DARK_BORDER
            },
            1,
        );
    }
    SetTextColor(
        mem_dc,
        if is_pinned {
            COLOR_PRO_RED
        } else if is_pin_hov {
            COLOR_TEXT_WHITE
        } else {
            COLOR_TEXT_MUTED
        },
    );
    let pin_w = to_wstring(GLYPH_PIN);
    let mut pin_rc = RECT {
        left: pin_x,
        top: pill_y,
        right: pin_x + tool_sz,
        bottom: pill_y + tool_sz,
    };
    DrawTextW(
        mem_dc,
        pin_w.as_ptr(),
        -1,
        &mut pin_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // Close / Hide Button
    let is_close_hov = hov == Some(HitArea::BtnClose);
    if is_close_hov {
        draw_round_rect(
            mem_dc,
            close_x,
            pill_y,
            close_x + tool_sz,
            pill_y + tool_sz,
            8,
            COLOR_RIOT_RED,
            COLOR_PRO_RED,
            1,
        );
    }
    SetTextColor(
        mem_dc,
        if is_close_hov {
            COLOR_TEXT_WHITE
        } else {
            COLOR_TEXT_SECONDARY
        },
    );
    let close_w = to_wstring(GLYPH_CLOSE);
    let mut close_rc = RECT {
        left: close_x,
        top: pill_y,
        right: close_x + tool_sz,
        bottom: pill_y + tool_sz,
    };
    DrawTextW(
        mem_dc,
        close_w.as_ptr(),
        -1,
        &mut close_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // =========================================================================
    // 2. ACTIVE ACCOUNT HERO BOX (y: 48..152)
    // =========================================================================
    let box_w = width - (PAD_X * 2);
    draw_round_rect(
        mem_dc,
        PAD_X,
        HERO_TOP_Y,
        PAD_X + box_w,
        HERO_TOP_Y + HERO_H,
        14,
        COLOR_HERO_BG,
        COLOR_HERO_BORDER,
        1,
    );

    let active_game_name = active_acc
        .as_ref()
        .map(|a| a.game_name.to_lowercase())
        .unwrap_or_default();
    let active_tag_line = active_acc
        .as_ref()
        .map(|a| a.tag_line.to_lowercase())
        .unwrap_or_default();

    let hero_display_name = if let Some(ref act) = active_acc {
        format!("{} #{}", act.game_name, act.tag_line)
    } else if let Some(first) = config.accounts.first() {
        format!("{} #{}", first.game_name, first.tag_line)
    } else {
        "No Account Selected".to_string()
    };

    // Hero Top Row: Icon + Active Account Name + "● ACTIVE" Pill
    SelectObject(mem_dc, font_sparkle as *mut _);
    SetTextColor(mem_dc, COLOR_PRO_RED);
    let h_icon = to_wstring("✦");
    let mut hi_rc = RECT {
        left: PAD_X + 14,
        top: HERO_TOP_Y + 14,
        right: PAD_X + 34,
        bottom: HERO_TOP_Y + 40,
    };
    DrawTextW(
        mem_dc,
        h_icon.as_ptr(),
        -1,
        &mut hi_rc,
        (DT_LEFT | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    SelectObject(mem_dc, font_hero_name as *mut _);
    SetTextColor(mem_dc, COLOR_TEXT_WHITE);
    let hn_w = to_wstring(&hero_display_name);
    let mut hn_rc = RECT {
        left: PAD_X + 36,
        top: HERO_TOP_Y + 14,
        right: width - PAD_X - 88,
        bottom: HERO_TOP_Y + 40,
    };
    DrawTextW(
        mem_dc,
        hn_w.as_ptr(),
        -1,
        &mut hn_rc,
        (DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS) as u32,
    );

    // Status Pill ("● ACTIVE" or "○ READY")
    let st_pill_w = 72;
    let st_pill_h = 22;
    let st_pill_x = PAD_X + box_w - st_pill_w - 14;
    let st_pill_y = HERO_TOP_Y + 16;
    let has_active = active_acc.is_some();

    draw_round_rect(
        mem_dc,
        st_pill_x,
        st_pill_y,
        st_pill_x + st_pill_w,
        st_pill_y + st_pill_h,
        11,
        if has_active {
            COLOR_ACTIVE_BADGE_BG
        } else {
            COLOR_BTN_DARK_BG
        },
        if has_active {
            COLOR_ACTIVE_BADGE_BD
        } else {
            COLOR_BTN_DARK_BORDER
        },
        1,
    );
    SelectObject(mem_dc, font_badge as *mut _);
    SetTextColor(
        mem_dc,
        if has_active {
            COLOR_GOOGLE_GREEN
        } else {
            COLOR_TEXT_SECONDARY
        },
    );
    let st_txt = to_wstring(if has_active { "● ACTIVE" } else { "○ READY" });
    let mut st_rc = RECT {
        left: st_pill_x,
        top: st_pill_y,
        right: st_pill_x + st_pill_w,
        bottom: st_pill_y + st_pill_h,
    };
    DrawTextW(
        mem_dc,
        st_txt.as_ptr(),
        -1,
        &mut st_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // Hero Bottom Row: [ ⇄ Switch Account ▾ ]  +  [ ⇄ Next ]
    let h_btn_y = HERO_TOP_Y + 54;
    let h_btn_h = 36;
    let next_btn_w = 82;
    let next_btn_x = PAD_X + box_w - 14 - next_btn_w;
    let sw_btn_x = PAD_X + 14;
    let sw_btn_w = next_btn_x - sw_btn_x - 8;

    let is_sw_hov = hov == Some(HitArea::BtnSwitchMenu);
    draw_round_rect(
        mem_dc,
        sw_btn_x,
        h_btn_y,
        sw_btn_x + sw_btn_w,
        h_btn_y + h_btn_h,
        9,
        if is_sw_hov {
            COLOR_BTN_DARK_HOVER
        } else {
            COLOR_BTN_DARK_BG
        },
        if is_sw_hov {
            COLOR_CARD_BORDER_HOVER
        } else {
            COLOR_BTN_DARK_BORDER
        },
        1,
    );
    SelectObject(mem_dc, font_btn as *mut _);
    SetTextColor(mem_dc, COLOR_TEXT_WHITE);
    let sw_txt = to_wstring("⇄   Switch Account   ▾");
    let mut sw_rc = RECT {
        left: sw_btn_x,
        top: h_btn_y,
        right: sw_btn_x + sw_btn_w,
        bottom: h_btn_y + h_btn_h,
    };
    DrawTextW(
        mem_dc,
        sw_txt.as_ptr(),
        -1,
        &mut sw_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    let is_next_hov = hov == Some(HitArea::BtnNextAccount);
    draw_round_rect(
        mem_dc,
        next_btn_x,
        h_btn_y,
        next_btn_x + next_btn_w,
        h_btn_y + h_btn_h,
        9,
        if is_next_hov {
            COLOR_RED_PILL_HOVER
        } else {
            COLOR_RED_PILL_BG
        },
        if is_next_hov {
            COLOR_PRO_RED
        } else {
            COLOR_RED_PILL_BORDER
        },
        1,
    );
    SetTextColor(
        mem_dc,
        if is_next_hov {
            COLOR_TEXT_WHITE
        } else {
            COLOR_RED_PILL_TEXT
        },
    );
    let next_txt = to_wstring("⇄  Next");
    let mut next_rc = RECT {
        left: next_btn_x,
        top: h_btn_y,
        right: next_btn_x + next_btn_w,
        bottom: h_btn_y + h_btn_h,
    };
    DrawTextW(
        mem_dc,
        next_txt.as_ptr(),
        -1,
        &mut next_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // =========================================================================
    // 3. COMPACT STREAM ACCOUNT CARDS (y: 162..450)
    // =========================================================================
    let clip_rgn = CreateRectRgn(0, LIST_START_Y - 2, width, FOOTER_START_Y - 4);
    SelectClipRgn(mem_dc, clip_rgn);

    if config.accounts.is_empty() {
        SelectObject(mem_dc, font_btn as *mut _);
        SetTextColor(mem_dc, COLOR_TEXT_SECONDARY);
        let empty_w = to_wstring("No saved accounts yet.\nClick '+ New' or '💾 Save' below to link an account.");
        let mut emp_rc = RECT {
            left: PAD_X + 16,
            top: LIST_START_Y + 70,
            right: width - PAD_X - 16,
            bottom: LIST_START_Y + 160,
        };
        DrawTextW(
            mem_dc,
            empty_w.as_ptr(),
            -1,
            &mut emp_rc,
            (DT_CENTER | DT_WORDBREAK) as u32,
        );
    } else {
        let card_w = box_w - 6; // Leave 6px on right for sleek scrollbar track
        for i in 0..config.accounts.len() {
            let acc = &config.accounts[i];
            let item_y = LIST_START_Y - scroll_offset + (i as i32 * (CARD_H + CARD_SPACING));

            if item_y + CARD_H < LIST_START_Y || item_y > FOOTER_START_Y {
                continue;
            }

            let is_current = !active_game_name.is_empty()
                && acc.game_name.to_lowercase() == active_game_name
                && acc.tag_line.to_lowercase() == active_tag_line;

            let is_hov = hov == Some(HitArea::Account(i));

            let bg_col = if is_hov {
                COLOR_CARD_HOVER
            } else if is_current {
                COLOR_CARD_ACTIVE
            } else {
                COLOR_CARD_BG
            };

            let bd_col = if is_current {
                COLOR_CARD_BORDER_ACTIVE
            } else if is_hov {
                COLOR_CARD_BORDER_HOVER
            } else {
                COLOR_CARD_BORDER
            };

            draw_round_rect(
                mem_dc,
                PAD_X,
                item_y,
                PAD_X + card_w,
                item_y + CARD_H,
                10,
                bg_col,
                bd_col,
                1,
            );

            // Left: Google AI Pro Sparkle ✦
            SelectObject(mem_dc, font_sparkle as *mut _);
            SetTextColor(
                mem_dc,
                if is_current {
                    COLOR_PRO_RED
                } else {
                    COLOR_RED_PILL_TEXT
                },
            );
            let sp_w = to_wstring("✦");
            let mut sp_rc = RECT {
                left: PAD_X + 12,
                top: item_y,
                right: PAD_X + 32,
                bottom: item_y + CARD_H,
            };
            DrawTextW(
                mem_dc,
                sp_w.as_ptr(),
                -1,
                &mut sp_rc,
                (DT_LEFT | DT_VCENTER | DT_SINGLELINE) as u32,
            );

            // Right Side Metrics: [ Server Badge ]  [ ◷ just now ]  [ 100% ]
            let pct_str = if is_current { "98%" } else { "100%" };
            let pct_w = to_wstring(pct_str);
            SelectObject(mem_dc, font_card_meta as *mut _);
            SetTextColor(mem_dc, COLOR_GOOGLE_GREEN);
            let pct_right = PAD_X + card_w - 12;
            let pct_left = pct_right - 36;
            let mut pct_rc = RECT {
                left: pct_left,
                top: item_y,
                right: pct_right,
                bottom: item_y + CARD_H,
            };
            DrawTextW(
                mem_dc,
                pct_w.as_ptr(),
                -1,
                &mut pct_rc,
                (DT_RIGHT | DT_VCENTER | DT_SINGLELINE) as u32,
            );

            // Age string: "◷ just now"
            let age_str = format!("◷ {}", format_compact_age(acc.last_used));
            let age_w = to_wstring(&age_str);
            SetTextColor(mem_dc, COLOR_RED_PILL_TEXT);
            let age_right = pct_left - 6;
            let age_left = age_right - 68;
            let mut age_rc = RECT {
                left: age_left,
                top: item_y,
                right: age_right,
                bottom: item_y + CARD_H,
            };
            DrawTextW(
                mem_dc,
                age_w.as_ptr(),
                -1,
                &mut age_rc,
                (DT_RIGHT | DT_VCENTER | DT_SINGLELINE) as u32,
            );

            // Server Region Pill (e.g. ME / EUW)
            SelectObject(mem_dc, font_server as *mut _);
            let srv_str = format!(" {} ", acc.region);
            let srv_w = to_wstring(&srv_str);
            let mut srv_sz: SIZE = std::mem::zeroed();
            GetTextExtentPoint32W(
                mem_dc,
                srv_w.as_ptr(),
                srv_w.len() as i32 - 1,
                &mut srv_sz,
            );
            let badge_w = (srv_sz.cx + 8).max(38);
            let badge_h = 22;
            let badge_right = age_left - 8;
            let badge_left = badge_right - badge_w;
            let badge_y = item_y + (CARD_H - badge_h) / 2;

            draw_round_rect(
                mem_dc,
                badge_left,
                badge_y,
                badge_right,
                badge_y + badge_h,
                6,
                COLOR_RED_PILL_BG,
                COLOR_RED_PILL_BORDER,
                1,
            );
            SetTextColor(mem_dc, COLOR_TEXT_WHITE);
            let mut srv_rc = RECT {
                left: badge_left,
                top: badge_y,
                right: badge_right,
                bottom: badge_y + badge_h,
            };
            DrawTextW(
                mem_dc,
                srv_w.as_ptr(),
                -1,
                &mut srv_rc,
                (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
            );

            // Account Name & Tag on Left (clipped before Server Badge)
            SelectObject(mem_dc, font_card_name as *mut _);
            SetTextColor(
                mem_dc,
                if is_current || is_hov {
                    COLOR_TEXT_WHITE
                } else {
                    rgb(225, 228, 235)
                },
            );
            let full_name = if acc.tag_line.is_empty() {
                acc.game_name.clone()
            } else {
                format!("{} #{}", acc.game_name, acc.tag_line)
            };
            let fn_w = to_wstring(&full_name);
            let mut fn_rc = RECT {
                left: PAD_X + 34,
                top: item_y,
                right: badge_left - 8,
                bottom: item_y + CARD_H,
            };
            DrawTextW(
                mem_dc,
                fn_w.as_ptr(),
                -1,
                &mut fn_rc,
                (DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS) as u32,
            );
        }

        // Scrollbar Track & Thumb on Right (matching reference image)
        let track_x = width - PAD_X - 3;
        let track_top = LIST_START_Y;
        let track_bottom = FOOTER_START_Y - 8;
        let visible_h = track_bottom - track_top;
        draw_round_rect(
            mem_dc,
            track_x,
            track_top,
            track_x + 4,
            track_bottom,
            2,
            rgb(16, 15, 22),
            rgb(16, 15, 22),
            0,
        );

        let total_content_h =
            (config.accounts.len() as i32) * (CARD_H + CARD_SPACING) - CARD_SPACING;
        if total_content_h > visible_h {
            let thumb_h = ((visible_h * visible_h) / total_content_h).max(28);
            let max_scroll = (total_content_h - visible_h).max(1);
            let thumb_y =
                track_top + ((scroll_offset * (visible_h - thumb_h)) / max_scroll);
            draw_round_rect(
                mem_dc,
                track_x,
                thumb_y,
                track_x + 4,
                thumb_y + thumb_h,
                2,
                COLOR_RED_PILL_BORDER,
                COLOR_RED_PILL_BORDER,
                0,
            );
        }
    }

    SelectClipRgn(mem_dc, 0 as HRGN);
    DeleteObject(clip_rgn as *mut _);

    // =========================================================================
    // 4. BOTTOM FOOTER BAR (y: 454..518)
    // =========================================================================
    fill_rect_color(
        mem_dc,
        PAD_X,
        FOOTER_START_Y - 2,
        width - PAD_X,
        FOOTER_START_Y - 1,
        COLOR_DIVIDER,
    );

    let ftr_cy = FOOTER_START_Y + 24;

    // Left: ⚡ Icon + "Auto Launch / (Launch on Switch)" + Toggle Switch
    SelectObject(mem_dc, font_icon as *mut _);
    SetTextColor(
        mem_dc,
        if auto_launch {
            COLOR_GOOGLE_GREEN
        } else {
            COLOR_TEXT_MUTED
        },
    );
    let bolt_w = to_wstring(GLYPH_BOLT);
    let mut bolt_rc = RECT {
        left: PAD_X,
        top: ftr_cy - 14,
        right: PAD_X + 18,
        bottom: ftr_cy + 14,
    };
    DrawTextW(
        mem_dc,
        bolt_w.as_ptr(),
        -1,
        &mut bolt_rc,
        (DT_LEFT | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    SelectObject(mem_dc, font_footer_main as *mut _);
    SetTextColor(mem_dc, COLOR_TEXT_WHITE);
    let al_t1 = to_wstring("Auto Launch");
    let mut al_r1 = RECT {
        left: PAD_X + 20,
        top: ftr_cy - 15,
        right: PAD_X + 118,
        bottom: ftr_cy + 1,
    };
    DrawTextW(
        mem_dc,
        al_t1.as_ptr(),
        -1,
        &mut al_r1,
        (DT_LEFT | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    SelectObject(mem_dc, font_footer_sub as *mut _);
    SetTextColor(mem_dc, COLOR_TEXT_SECONDARY);
    let al_t2 = to_wstring("(Launch on Switch)");
    let mut al_r2 = RECT {
        left: PAD_X + 20,
        top: ftr_cy + 1,
        right: PAD_X + 122,
        bottom: ftr_cy + 16,
    };
    DrawTextW(
        mem_dc,
        al_t2.as_ptr(),
        -1,
        &mut al_r2,
        (DT_LEFT | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // Interactive Toggle Switch Pill
    let sw_x = PAD_X + 124;
    let sw_y = ftr_cy - 10;
    let sw_w = 38;
    let sw_h = 20;
    draw_round_rect(
        mem_dc,
        sw_x,
        sw_y,
        sw_x + sw_w,
        sw_y + sw_h,
        10,
        if auto_launch {
            COLOR_GOOGLE_GREEN
        } else {
            rgb(34, 30, 42)
        },
        if auto_launch {
            COLOR_GOOGLE_GREEN
        } else {
            rgb(58, 52, 68)
        },
        1,
    );
    let knob_cx = if auto_launch {
        sw_x + sw_w - 10
    } else {
        sw_x + 10
    };
    draw_circle(mem_dc, knob_cx, sw_y + 10, 7, COLOR_TEXT_WHITE);

    // Right Action Buttons: [ 💾 Save ]  [ + New ]  [ ⏻ ]
    let act_h = 32;
    let act_y = ftr_cy - (act_h / 2);

    // 1. Power / Kill Riot Button [ ⏻ ]
    let kill_w = 32;
    let kill_x = width - PAD_X - kill_w;
    let is_kill_hov = hov == Some(HitArea::BtnKillRiot);
    draw_round_rect(
        mem_dc,
        kill_x,
        act_y,
        kill_x + kill_w,
        act_y + act_h,
        8,
        if is_kill_hov {
            COLOR_RIOT_RED
        } else {
            rgb(38, 12, 18)
        },
        if is_kill_hov {
            COLOR_PRO_RED
        } else {
            rgb(90, 24, 36)
        },
        1,
    );
    SelectObject(mem_dc, font_icon as *mut _);
    SetTextColor(
        mem_dc,
        if is_kill_hov {
            COLOR_TEXT_WHITE
        } else {
            COLOR_PRO_RED
        },
    );
    let pwr_w = to_wstring(GLYPH_POWER);
    let mut pwr_rc = RECT {
        left: kill_x,
        top: act_y,
        right: kill_x + kill_w,
        bottom: act_y + act_h,
    };
    DrawTextW(
        mem_dc,
        pwr_w.as_ptr(),
        -1,
        &mut pwr_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // 2. [ + New ] Button
    let new_w = 58;
    let new_x = kill_x - 5 - new_w;
    let is_new_hov = hov == Some(HitArea::AddAccount);
    draw_round_rect(
        mem_dc,
        new_x,
        act_y,
        new_x + new_w,
        act_y + act_h,
        8,
        if is_new_hov {
            COLOR_RED_PILL_HOVER
        } else {
            COLOR_RED_PILL_BG
        },
        if is_new_hov {
            COLOR_PRO_RED
        } else {
            COLOR_RED_PILL_BORDER
        },
        1,
    );
    SelectObject(mem_dc, font_footer_main as *mut _);
    SetTextColor(
        mem_dc,
        if is_new_hov {
            COLOR_TEXT_WHITE
        } else {
            COLOR_RED_PILL_TEXT
        },
    );
    let new_txt = to_wstring("+ New");
    let mut new_rc = RECT {
        left: new_x,
        top: act_y,
        right: new_x + new_w,
        bottom: act_y + act_h,
    };
    DrawTextW(
        mem_dc,
        new_txt.as_ptr(),
        -1,
        &mut new_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // 3. [ 💾 Save ] Button
    let save_w = 58;
    let save_x = new_x - 5 - save_w;
    let is_save_hov = hov == Some(HitArea::BtnSaveCurrent);
    draw_round_rect(
        mem_dc,
        save_x,
        act_y,
        save_x + save_w,
        act_y + act_h,
        8,
        if is_save_hov {
            COLOR_BTN_DARK_HOVER
        } else {
            COLOR_BTN_DARK_BG
        },
        if is_save_hov {
            COLOR_CARD_BORDER_HOVER
        } else {
            COLOR_BTN_DARK_BORDER
        },
        1,
    );
    SetTextColor(mem_dc, COLOR_TEXT_WHITE);
    let save_txt = to_wstring("Save");
    let mut save_rc = RECT {
        left: save_x,
        top: act_y,
        right: save_x + save_w,
        bottom: act_y + act_h,
    };
    DrawTextW(
        mem_dc,
        save_txt.as_ptr(),
        -1,
        &mut save_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // Optional Micro Feedback Toast Line at bottom edge
    if !feedback.is_empty() {
        SelectObject(mem_dc, font_footer_sub as *mut _);
        SetTextColor(
            mem_dc,
            if is_err {
                COLOR_PRO_RED
            } else {
                COLOR_RED_PILL_TEXT
            },
        );
        let fb_w = to_wstring(&feedback);
        let mut fb_rc = RECT {
            left: PAD_X,
            top: height - 18,
            right: width - PAD_X,
            bottom: height - 2,
        };
        DrawTextW(
            mem_dc,
            fb_w.as_ptr(),
            -1,
            &mut fb_rc,
            (DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS) as u32,
        );
    }

    // Blit to screen
    BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

    // Clean up GDI objects
    SelectObject(mem_dc, old_bmp);
    DeleteObject(mem_bmp as *mut _);
    DeleteDC(mem_dc);

    DeleteObject(font_pill as *mut _);
    DeleteObject(font_hero_name as *mut _);
    DeleteObject(font_badge as *mut _);
    DeleteObject(font_btn as *mut _);
    DeleteObject(font_card_name as *mut _);
    DeleteObject(font_card_meta as *mut _);
    DeleteObject(font_server as *mut _);
    DeleteObject(font_footer_main as *mut _);
    DeleteObject(font_footer_sub as *mut _);
    DeleteObject(font_sparkle as *mut _);
    DeleteObject(font_icon as *mut _);
}

// =========================================================================
// MOUSE & INTERACTION HANDLING
// =========================================================================

unsafe fn handle_mouse_move(hwnd: HWND, x: i32, y: i32) {
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    let width = rect.right - rect.left;

    let need_track = {
        if let Some(state_arc) = GUI_STATE.get() {
            if let Ok(mut state) = state_arc.lock() {
                if !state.tracking_mouse {
                    state.tracking_mouse = true;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    };
    if need_track {
        let mut tme = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        TrackMouseEvent(&mut tme);
    }

    let pill_y = 10;
    let pill_h = 28;
    let riot_x = PAD_X;
    let riot_w = 86;
    let val_x = riot_x + riot_w + 6;
    let val_w = 66;
    let lol_x = val_x + val_w + 4;
    let lol_w = 64;

    let tool_sz = 28;
    let close_x = width - PAD_X - tool_sz;
    let pin_x = close_x - tool_sz - 4;
    let sync_x = pin_x - tool_sz - 4;

    let box_w = width - (PAD_X * 2);
    let h_btn_y = HERO_TOP_Y + 54;
    let h_btn_h = 36;
    let next_btn_w = 82;
    let next_btn_x = PAD_X + box_w - 14 - next_btn_w;
    let sw_btn_x = PAD_X + 14;
    let sw_btn_w = next_btn_x - sw_btn_x - 8;

    let ftr_cy = FOOTER_START_Y + 24;
    let act_h = 32;
    let act_y = ftr_cy - (act_h / 2);
    let kill_w = 32;
    let kill_x = width - PAD_X - kill_w;
    let new_w = 58;
    let new_x = kill_x - 5 - new_w;
    let save_w = 58;
    let save_x = new_x - 5 - save_w;

    let mut new_hit: Option<HitArea> = None;

    if y >= 0 && y <= TOPBAR_H {
        if x >= riot_x && x <= riot_x + riot_w && y >= pill_y && y <= pill_y + pill_h {
            new_hit = Some(HitArea::TabRiot);
        } else if x >= val_x && x <= val_x + val_w && y >= pill_y && y <= pill_y + pill_h {
            new_hit = Some(HitArea::TabVal);
        } else if x >= lol_x && x <= lol_x + lol_w && y >= pill_y && y <= pill_y + pill_h {
            new_hit = Some(HitArea::TabLol);
        } else if x >= sync_x && x <= sync_x + tool_sz && y >= pill_y && y <= pill_y + tool_sz {
            new_hit = Some(HitArea::BtnSync);
        } else if x >= pin_x && x <= pin_x + tool_sz && y >= pill_y && y <= pill_y + tool_sz {
            new_hit = Some(HitArea::BtnPin);
        } else if x >= close_x && x <= close_x + tool_sz && y >= pill_y && y <= pill_y + tool_sz {
            new_hit = Some(HitArea::BtnClose);
        } else {
            new_hit = Some(HitArea::TitleBar);
        }
    } else if y >= h_btn_y && y <= h_btn_y + h_btn_h {
        if x >= sw_btn_x && x <= sw_btn_x + sw_btn_w {
            new_hit = Some(HitArea::BtnSwitchMenu);
        } else if x >= next_btn_x && x <= next_btn_x + next_btn_w {
            new_hit = Some(HitArea::BtnNextAccount);
        }
    } else if y >= LIST_START_Y && y < FOOTER_START_Y {
        let (acc_count, scroll_offset) = {
            if let Some(state_arc) = GUI_STATE.get() {
                if let Ok(state) = state_arc.lock() {
                    (
                        state.storage.load_config().accounts.len(),
                        state.scroll_offset,
                    )
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            }
        };

        let card_w = box_w - 6;
        for i in 0..acc_count {
            let item_y = LIST_START_Y - scroll_offset + (i as i32 * (CARD_H + CARD_SPACING));
            if x >= PAD_X && x <= PAD_X + card_w && y >= item_y && y <= item_y + CARD_H {
                new_hit = Some(HitArea::Account(i));
                break;
            }
        }
    } else if y >= FOOTER_START_Y {
        if x >= kill_x && x <= kill_x + kill_w && y >= act_y && y <= act_y + act_h {
            new_hit = Some(HitArea::BtnKillRiot);
        } else if x >= new_x && x <= new_x + new_w && y >= act_y && y <= act_y + act_h {
            new_hit = Some(HitArea::AddAccount);
        } else if x >= save_x && x <= save_x + save_w && y >= act_y && y <= act_y + act_h {
            new_hit = Some(HitArea::BtnSaveCurrent);
        } else if x >= PAD_X && x <= PAD_X + 168 && y >= ftr_cy - 16 && y <= ftr_cy + 16 {
            new_hit = Some(HitArea::ToggleAutoLaunch);
        }
    }

    let mut needs_redraw = false;
    if let Some(state_arc) = GUI_STATE.get() {
        if let Ok(mut state) = state_arc.lock() {
            if state.hovered != new_hit {
                state.hovered = new_hit;
                needs_redraw = true;
            }
        }
    }

    if needs_redraw {
        InvalidateRect(hwnd, null_mut(), 0);
    }
}

unsafe fn handle_right_click(hwnd: HWND, x: i32, y: i32) {
    handle_mouse_move(hwnd, x, y);
    let hit = GUI_STATE
        .get()
        .and_then(|s| s.lock().ok().and_then(|st| st.hovered));

    if let Some(HitArea::Account(idx)) = hit {
        let mut pt: POINT = std::mem::zeroed();
        GetCursorPos(&mut pt);
        let hmenu = CreatePopupMenu();
        let sw_str = to_wstring("⚡ Switch to Account");
        let del_str = to_wstring("🗑 Delete Account");
        AppendMenuW(hmenu, MF_STRING, ID_CTX_SWITCH_BASE + idx, sw_str.as_ptr());
        AppendMenuW(hmenu, MF_SEPARATOR, 0, null_mut());
        AppendMenuW(hmenu, MF_STRING, ID_CTX_DELETE_BASE + idx, del_str.as_ptr());
        TrackPopupMenu(
            hmenu,
            TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            hwnd,
            null_mut(),
        );
        DestroyMenu(hmenu);
    }
}

unsafe fn handle_mouse_down(hwnd: HWND, x: i32, y: i32) {
    handle_mouse_move(hwnd, x, y);

    let (hit, is_busy) = {
        if let Some(state_arc) = GUI_STATE.get() {
            if let Ok(state) = state_arc.lock() {
                (state.hovered, state.is_busy)
            } else {
                (None, false)
            }
        } else {
            (None, false)
        }
    };

    if let Some(h) = hit {
        match h {
            HitArea::TitleBar => {
                ReleaseCapture();
                SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as WPARAM, 0);
            }
            HitArea::BtnClose => {
                hide_to_tray(hwnd);
            }
            HitArea::BtnPin => {
                let mut pinned = true;
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        state.is_pinned = !state.is_pinned;
                        pinned = state.is_pinned;
                        state.feedback_text = if pinned {
                            "📌 Flyout pinned on top".to_string()
                        } else {
                            "📌 Flyout auto-hide enabled".to_string()
                        };
                        state.feedback_is_error = false;
                    }
                }
                SetWindowPos(
                    hwnd,
                    if pinned {
                        HWND_TOPMOST
                    } else {
                        HWND_NOTOPMOST
                    },
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE,
                );
                InvalidateRect(hwnd, null_mut(), 0);
            }
            HitArea::ToggleAutoLaunch => {
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        state.auto_launch = !state.auto_launch;
                    }
                }
                InvalidateRect(hwnd, null_mut(), 0);
            }
            HitArea::TabRiot => {
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        let _ = state.riot.launch("riot_client");
                        state.feedback_text = "✦ Launched Riot Client".to_string();
                        state.feedback_is_error = false;
                    }
                }
                InvalidateRect(hwnd, null_mut(), 0);
            }
            HitArea::TabVal => {
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        let _ = state.riot.launch("valorant");
                        state.feedback_text = "✦ Launching VALORANT...".to_string();
                        state.feedback_is_error = false;
                    }
                }
                InvalidateRect(hwnd, null_mut(), 0);
            }
            HitArea::TabLol => {
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        let _ = state.riot.launch("league");
                        state.feedback_text = "✦ Launching League of Legends...".to_string();
                        state.feedback_is_error = false;
                    }
                }
                InvalidateRect(hwnd, null_mut(), 0);
            }
            HitArea::BtnSync | HitArea::BtnSaveCurrent => {
                if is_busy {
                    return;
                }
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        state.storage.import_external_profiles();
                        if let Some(act) = state.riot.get_active_account() {
                            match state.storage.save_active_account(
                                &act,
                                None,
                                &state.riot.settings_yaml_path,
                            ) {
                                Ok(p) => {
                                    state.active_account = Some(act);
                                    state.feedback_text =
                                        format!("✦ Saved session: {} #{}", p.game_name, p.tag_line);
                                    state.feedback_is_error = false;
                                }
                                Err(e) => {
                                    state.feedback_text = e;
                                    state.feedback_is_error = true;
                                }
                            }
                        } else {
                            state.feedback_text =
                                "No active persistent session in Riot Client.".to_string();
                            state.feedback_is_error = true;
                        }
                    }
                }
                InvalidateRect(hwnd, null_mut(), 0);
            }
            HitArea::BtnSwitchMenu => {
                let accounts = {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(state) = state_arc.lock() {
                            state.storage.load_config().accounts
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    }
                };
                if accounts.is_empty() {
                    return;
                }
                let mut pt = POINT {
                    x: PAD_X + 14,
                    y: HERO_TOP_Y + 90,
                };
                ClientToScreen(hwnd, &mut pt);
                let hmenu = CreatePopupMenu();
                for (i, acc) in accounts.iter().enumerate() {
                    let label = to_wstring(&format!(
                        "✦  {} #{}  [{}]",
                        acc.game_name, acc.tag_line, acc.region
                    ));
                    AppendMenuW(hmenu, MF_STRING, ID_CTX_SWITCH_BASE + i, label.as_ptr());
                }
                TrackPopupMenu(
                    hmenu,
                    TPM_LEFTALIGN | TPM_TOPALIGN,
                    pt.x,
                    pt.y,
                    0,
                    hwnd,
                    null_mut(),
                );
                DestroyMenu(hmenu);
            }
            HitArea::BtnNextAccount => {
                trigger_next_account(hwnd);
            }
            HitArea::Account(idx) => {
                trigger_account_switch(hwnd, idx);
            }
            HitArea::AddAccount => {
                if is_busy {
                    return;
                }
                let (riot, storage) = {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(mut state) = state_arc.lock() {
                            state.is_busy = true;
                            state.feedback_text =
                                "⚡ Preparing clean login screen...".to_string();
                            state.feedback_is_error = false;
                            (state.riot.clone(), state.storage.clone())
                        } else {
                            return;
                        }
                    } else {
                        return;
                    }
                };
                InvalidateRect(hwnd, null_mut(), 0);
                UpdateWindow(hwnd);

                let hwnd_val = hwnd as isize;
                std::thread::spawn(move || {
                    storage.backup_current_active_session(&riot);
                    riot.prepare_clean_login();
                    let _ = riot.launch("riot_client");

                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(mut state) = state_arc.lock() {
                            state.is_busy = false;
                            state.active_account = None;
                            state.feedback_text =
                                "✦ Sign in with 'Stay signed in' checked!".to_string();
                            state.feedback_is_error = false;
                        }
                    }
                    PostMessageW(hwnd_val as HWND, WM_APP_ASYNC_DONE, 0, 0);
                });
            }
            HitArea::BtnKillRiot => {
                if is_busy {
                    return;
                }
                let (riot, storage) = {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(mut state) = state_arc.lock() {
                            state.is_busy = true;
                            state.feedback_text = "⏻ Closing Riot Client...".to_string();
                            state.feedback_is_error = false;
                            (state.riot.clone(), state.storage.clone())
                        } else {
                            return;
                        }
                    } else {
                        return;
                    }
                };
                InvalidateRect(hwnd, null_mut(), 0);
                UpdateWindow(hwnd);

                let hwnd_val = hwnd as isize;
                std::thread::spawn(move || {
                    storage.backup_current_active_session(&riot);
                    RiotManager::kill_riot_processes();
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(mut state) = state_arc.lock() {
                            state.is_busy = false;
                            state.feedback_text = "✦ All Riot processes closed.".to_string();
                            state.feedback_is_error = false;
                        }
                    }
                    PostMessageW(hwnd_val as HWND, WM_APP_ASYNC_DONE, 0, 0);
                });
            }
        }
    }
}
