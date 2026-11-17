use crate::riot::{ActiveAccount, RiotManager};
use crate::storage::StorageManager;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex, OnceLock};
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
const ID_TRAY_EXIT: usize = 2002;
const TIMER_AUTODETECT: usize = 1001;

/// Helper to construct Win32 COLORREF (0x00BBGGRR) from standard RGB
const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

// =========================================================================
// GOOGLE AI PRO — DEEP OLED BLACK & CRIMSON RED PALETTE
// =========================================================================
const COLOR_OLED_BG: u32            = rgb(3, 3, 5);       // #030305 True Infinite OLED Black
const COLOR_TOPBAR_BG: u32          = rgb(8, 7, 10);      // #08070A Elevated Top App Bar
const COLOR_DOCK_BG: u32            = rgb(7, 6, 9);       // #070609 Bottom Action Dock
const COLOR_OUTER_BORDER: u32       = rgb(48, 18, 26);    // #30121A Subtle Crimson-Obsidian Frame
const COLOR_DIVIDER: u32            = rgb(26, 22, 28);    // #1A161C Hairline Structural Divider

// Card Surfaces
const COLOR_CARD_BG: u32            = rgb(11, 10, 14);    // #0B0A0E Google AI Pro Obsidian Card
const COLOR_CARD_HOVER: u32         = rgb(22, 11, 16);    // #160B10 Crimson-Tinted OLED Hover
const COLOR_CARD_ACTIVE: u32        = rgb(19, 9, 13);     // #13090D Active Session Surface
const COLOR_CARD_BORDER: u32        = rgb(28, 25, 33);    // #1C1921 Idle Card Border
const COLOR_CARD_BORDER_ACTIVE: u32 = rgb(150, 22, 48);   // #961630 Active Card Crimson Border
const COLOR_CARD_BORDER_HOVER: u32  = rgb(255, 32, 68);   // #FF2044 Electric Red Hover Border

// Accents & Badges
const COLOR_PRO_RED: u32            = rgb(255, 30, 66);   // #FF1E42 Google AI Pro Crimson Red
const COLOR_RIOT_RED: u32           = rgb(235, 0, 41);    // #EB0029 Official Riot Red
const COLOR_PRO_BADGE_BG: u32       = rgb(38, 8, 16);     // #260810 Pro Pill Fill
const COLOR_PRO_BADGE_BORDER: u32   = rgb(190, 24, 52);   // #BE1834 Pro Pill Border
const COLOR_PRO_BADGE_TEXT: u32     = rgb(255, 90, 115);  // #FF5A73 Pro Pill Text

const COLOR_SERVER_BG: u32          = rgb(26, 8, 14);     // #1A080E Server Badge Fill
const COLOR_SERVER_BORDER: u32      = rgb(105, 20, 38);   // #691426 Server Badge Border
const COLOR_SERVER_TEXT: u32        = rgb(255, 85, 110);  // #FF556E Server Badge Text

const COLOR_GOOGLE_GREEN: u32       = rgb(52, 199, 89);   // #34C759 Active Session LED Green

// Typography Colors (Google Material 3 Dark)
const COLOR_TEXT_WHITE: u32         = rgb(248, 249, 250); // #F8F9FA Crisp Primary White
const COLOR_TEXT_SECONDARY: u32     = rgb(154, 160, 166); // #9AA0A6 Secondary Label Gray
const COLOR_TEXT_MUTED: u32         = rgb(104, 108, 118); // #686C76 Muted Caption Gray

// Layout Geometry Constants
const WIN_WIDTH: i32       = 480;
const WIN_HEIGHT: i32      = 580;
const TOPBAR_H: i32        = 52;
const HEADER_END_Y: i32    = 132;
const DOCK_START_Y: i32    = 480;
const CARD_X: i32          = 22;
const CARD_H: i32          = 82;
const CARD_SPACING: i32    = 10;

// Win32 Icon constants
const IMAGE_ICON_TYPE: u32      = 1;
const LR_LOADFROMFILE_FLAG: u32 = 0x00000010;
const DI_NORMAL_FLAG: u32       = 0x0003;

// Authentic Riot Games Official Typography
const FONT_BEAUFORT_BOLD: &[u8]   = include_bytes!("../assets/fonts/BeaufortforLOL-Bold.ttf");
const FONT_BEAUFORT_MEDIUM: &[u8] = include_bytes!("../assets/fonts/BeaufortforLOL-Medium.ttf");

// Unicode Vector Glyphs (Segoe Fluent Icons / Segoe MDL2 Assets)
const GLYPH_PLAY: &str     = "\u{E768}";
const GLYPH_TRASH: &str    = "\u{E74D}";
const GLYPH_MINIMIZE: &str = "\u{E921}";
const GLYPH_CLOSE: &str    = "\u{E8BB}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitArea {
    TitleBar,
    BtnClose,
    BtnMinimize,
    BtnSync,
    Account(usize),
    PlayBtn(usize),
    Delete(usize),
    AddAccount,
}

struct GuiState {
    riot: RiotManager,
    storage: StorageManager,
    active_account: Option<ActiveAccount>,
    hovered: Option<HitArea>,
    feedback_text: String,
    feedback_is_error: bool,
    is_busy: bool,
    tracking_mouse: bool,
    official_icon: HICON,
    app_icon_small: HICON,
    account_icons: HashMap<String, HICON>,
    icons_dir: PathBuf,
    scroll_offset: i32,
}

unsafe impl Send for GuiState {}
unsafe impl Sync for GuiState {}

static GUI_STATE: OnceLock<Arc<Mutex<GuiState>>> = OnceLock::new();

pub fn to_wstring(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Loads the official Riot Games icon from disk
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

/// Loads application/window icon (the dark black icon from embedded PE resources or assets/app.ico)
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
    let mut ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
    ex_style &= !(WS_EX_TOOLWINDOW as i32);
    ex_style |= WS_EX_APPWINDOW as i32;
    SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style);
    ShowWindow(hwnd, SW_RESTORE);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
}

/// Automatically detects any active Riot account and keeps its rotating refresh token synced
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

            // Always sync the latest rotating refresh token to the profile file
            if let Ok(p) = state
                .storage
                .save_active_account(act, None, &state.riot.settings_yaml_path)
            {
                if !already_saved {
                    state.feedback_text =
                        format!("✦ Auto-linked new account: {} #{}", p.game_name, p.tag_line);
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
        // Register authentic Riot Games fonts into private process memory
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
        let app_icon_small = load_app_window_icon_sized(20);
        let official_riot_icon = load_official_riot_icon_sized(46);

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

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let start_x = (screen_w - WIN_WIDTH) / 2;
        let start_y = (screen_h - WIN_HEIGHT) / 2;

        let window_title = to_wstring("Riot Fast Login");
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
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

        // Enable Windows 11 Dark Mode & Smooth Hardware Rounded Corners
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
            33, // DWMWA_WINDOW_CORNER_PREFERENCE
            &corner_pref as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );
        let border_col: u32 = COLOR_OUTER_BORDER;
        DwmSetWindowAttribute(
            hwnd,
            34, // DWMWA_BORDER_COLOR
            &border_col as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        let riot = RiotManager::new();
        let storage = StorageManager::new();
        let icons_dir = storage.profiles_dir.parent().unwrap().join("icons");
        let _ = std::fs::create_dir_all(&icons_dir);

        let mut state = GuiState {
            riot,
            storage,
            active_account: None,
            hovered: None,
            feedback_text: "✦ Google AI Pro Session Engine • Ready to switch".to_string(),
            feedback_is_error: false,
            is_busy: false,
            tracking_mouse: false,
            official_icon: official_riot_icon,
            app_icon_small,
            account_icons: HashMap::new(),
            icons_dir,
            scroll_offset: 0,
        };

        check_and_autosave(&mut state);

        let _ = GUI_STATE.set(Arc::new(Mutex::new(state)));

        SetTimer(hwnd, TIMER_AUTODETECT, 2000, None);

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
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
                        let fg = GetForegroundWindow();
                        if fg == hwnd {
                            hide_to_tray(hwnd);
                        } else {
                            SetForegroundWindow(hwnd);
                        }
                    }
                }
                WM_RBUTTONUP => {
                    let mut pt: POINT = std::mem::zeroed();
                    GetCursorPos(&mut pt);
                    SetForegroundWindow(hwnd);

                    let hmenu = CreatePopupMenu();
                    let open_str = to_wstring("Open Riot Fast Login PRO");
                    let exit_str = to_wstring("Exit");
                    AppendMenuW(hmenu, MF_STRING, ID_TRAY_OPEN, open_str.as_ptr());
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
            match id {
                ID_TRAY_OPEN => {
                    restore_from_tray(hwnd);
                }
                ID_TRAY_EXIT => {
                    DestroyWindow(hwnd);
                }
                _ => {}
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
                    let visible_h = DOCK_START_Y - HEADER_END_Y - 12;
                    let max_scroll = (total_content_h - visible_h).max(0);

                    let step = (delta / 120) * 46;
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

// =========================================================================
// GOOGLE AI PRO VECTOR CONTROLS
// =========================================================================

unsafe fn draw_pro_play_button(
    hdc: HDC,
    cx: i32,
    cy: i32,
    radius: i32,
    is_hover: bool,
    is_current: bool,
    icon_font: HFONT,
) {
    let bg_col = if is_hover {
        COLOR_PRO_RED
    } else if is_current {
        COLOR_RIOT_RED
    } else {
        rgb(32, 10, 16)
    };
    let border_col = if is_hover {
        rgb(255, 110, 130)
    } else if is_current {
        COLOR_PRO_RED
    } else {
        rgb(110, 20, 40)
    };

    draw_round_rect(
        hdc,
        cx - radius,
        cy - radius,
        cx + radius,
        cy + radius,
        radius * 2,
        bg_col,
        border_col,
        if is_hover { 2 } else { 1 },
    );

    SelectObject(hdc, icon_font as *mut _);
    SetTextColor(
        hdc,
        if is_hover || is_current {
            COLOR_TEXT_WHITE
        } else {
            COLOR_PRO_BADGE_TEXT
        },
    );
    let play_w = to_wstring(GLYPH_PLAY);
    let mut play_rect = RECT {
        left: cx - radius + 2,
        top: cy - radius,
        right: cx + radius + 2,
        bottom: cy + radius,
    };
    DrawTextW(
        hdc,
        play_w.as_ptr(),
        -1,
        &mut play_rect,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );
}

unsafe fn draw_pro_trash_button(
    hdc: HDC,
    cx: i32,
    cy: i32,
    size: i32,
    is_hover: bool,
    icon_font: HFONT,
) {
    let half = size / 2;
    let bg_col = if is_hover {
        rgb(45, 10, 18)
    } else {
        rgb(16, 14, 19)
    };
    let border_col = if is_hover {
        COLOR_PRO_RED
    } else {
        rgb(38, 32, 42)
    };

    draw_round_rect(
        hdc,
        cx - half,
        cy - half,
        cx + half,
        cy + half,
        8,
        bg_col,
        border_col,
        1,
    );

    SelectObject(hdc, icon_font as *mut _);
    SetTextColor(
        hdc,
        if is_hover {
            COLOR_PRO_RED
        } else {
            COLOR_TEXT_MUTED
        },
    );
    let trash_w = to_wstring(GLYPH_TRASH);
    let mut trash_rect = RECT {
        left: cx - half,
        top: cy - half,
        right: cx + half,
        bottom: cy + half,
    };
    DrawTextW(
        hdc,
        trash_w.as_ptr(),
        -1,
        &mut trash_rect,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );
}

unsafe fn draw_status_dot(hdc: HDC, cx: i32, cy: i32, color: u32, halo_color: u32) {
    let null_pen = CreatePen(PS_NULL as i32, 0, 0);
    let halo_brush = CreateSolidBrush(halo_color);
    let old_p = SelectObject(hdc, null_pen as *mut _);
    let old_b = SelectObject(hdc, halo_brush as *mut _);
    Ellipse(hdc, cx - 6, cy - 6, cx + 7, cy + 7);

    let core_brush = CreateSolidBrush(color);
    SelectObject(hdc, core_brush as *mut _);
    Ellipse(hdc, cx - 3, cy - 3, cx + 4, cy + 4);

    SelectObject(hdc, old_b);
    SelectObject(hdc, old_p);
    DeleteObject(halo_brush as *mut _);
    DeleteObject(core_brush as *mut _);
    DeleteObject(null_pen as *mut _);
}

// =========================================================================
// MAIN RENDER LOOP — GOOGLE AI PRO OLED BLACK & RED
// =========================================================================

unsafe fn render_riot_client(hwnd: HWND, hdc: HDC) {
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    let mem_dc = CreateCompatibleDC(hdc);
    let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
    let old_bmp = SelectObject(mem_dc, mem_bmp as *mut _);

    // 1. Fill Deep OLED Black Canvas
    fill_rect_color(mem_dc, 0, 0, width, height, COLOR_OLED_BG);

    // 2. Top App Bar & Bottom Dock Surfaces
    fill_rect_color(mem_dc, 0, 0, width, TOPBAR_H, COLOR_TOPBAR_BG);
    fill_rect_color(mem_dc, 0, DOCK_START_Y, width, height, COLOR_DOCK_BG);

    // Structural Hairline Dividers + Crimson Glow Accent
    fill_rect_color(mem_dc, 0, TOPBAR_H, width, TOPBAR_H + 1, COLOR_DIVIDER);
    fill_rect_color(mem_dc, CARD_X, TOPBAR_H, CARD_X + 115, TOPBAR_H + 2, COLOR_PRO_RED);
    fill_rect_color(mem_dc, 0, DOCK_START_Y, width, DOCK_START_Y + 1, COLOR_DIVIDER);

    // Outer Window Frame
    let border_pen = CreatePen(PS_SOLID as i32, 1, COLOR_OUTER_BORDER);
    let old_p = SelectObject(mem_dc, border_pen as *mut _);
    let old_b = SelectObject(mem_dc, GetStockObject(NULL_BRUSH as i32));
    Rectangle(mem_dc, 0, 0, width, height);
    SelectObject(mem_dc, old_b);
    SelectObject(mem_dc, old_p);
    DeleteObject(border_pen as *mut _);

    // Typography
    let font_brand = create_riot_font(14, true);
    let font_pro_pill = create_riot_font(11, true);
    let font_header = create_riot_font(25, true);
    let font_sub = create_riot_sub_font(13, false);
    let font_btn_bold = create_riot_sub_font(14, true);
    let font_name_large = create_riot_font(20, true);
    let font_tag = create_riot_sub_font(14, false);
    let font_server_badge = create_riot_font(22, true); // 2x Scaled Server Font
    let font_meta = create_riot_sub_font(12, true);

    // Vector Icon Fonts
    let font_icon_play = create_vector_icon_font(14);
    let font_icon_trash = create_vector_icon_font(13);
    let font_icon_window = create_vector_icon_font(11);

    SetBkMode(mem_dc, TRANSPARENT as i32);

    let (active_acc, config, hov, feedback, is_err, is_busy, scroll_offset, app_icon_small) = {
        if let Some(state_arc) = GUI_STATE.get() {
            if let Ok(state) = state_arc.lock() {
                let conf = state.storage.load_config();
                (
                    state.active_account.clone(),
                    conf,
                    state.hovered,
                    state.feedback_text.clone(),
                    state.feedback_is_error,
                    state.is_busy,
                    state.scroll_offset,
                    state.app_icon_small,
                )
            } else {
                return;
            }
        } else {
            return;
        }
    };

    // =========================================================================
    // 3. GOOGLE AI PRO TOP BAR (0..52px)
    // =========================================================================
    let logo_x = CARD_X;
    let logo_y = 16;
    if app_icon_small != 0 as HICON {
        DrawIconEx(
            mem_dc,
            logo_x,
            logo_y,
            app_icon_small,
            20,
            20,
            0,
            0 as HBRUSH,
            DI_NORMAL_FLAG,
        );
    } else {
        draw_round_rect(
            mem_dc,
            logo_x,
            logo_y,
            logo_x + 20,
            logo_y + 20,
            5,
            COLOR_RIOT_RED,
            COLOR_PRO_RED,
            1,
        );
    }

    SelectObject(mem_dc, font_brand as *mut _);
    SetTextColor(mem_dc, COLOR_TEXT_WHITE);
    let brand_w = to_wstring("RIOT FAST LOGIN");
    let mut brand_sz: SIZE = std::mem::zeroed();
    GetTextExtentPoint32W(
        mem_dc,
        brand_w.as_ptr(),
        brand_w.len() as i32 - 1,
        &mut brand_sz,
    );
    let brand_x = logo_x + 28;
    let mut brand_rect = RECT {
        left: brand_x,
        top: 0,
        right: brand_x + brand_sz.cx + 4,
        bottom: TOPBAR_H,
    };
    DrawTextW(
        mem_dc,
        brand_w.as_ptr(),
        -1,
        &mut brand_rect,
        (DT_LEFT | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // Google AI Pro "✦ PRO" Capsule Badge positioned dynamically right after brand title
    let pro_x = brand_x + brand_sz.cx + 10;
    let pro_y = 16;
    let pro_w = 50;
    let pro_h = 20;
    draw_round_rect(
        mem_dc,
        pro_x,
        pro_y,
        pro_x + pro_w,
        pro_y + pro_h,
        10,
        COLOR_PRO_BADGE_BG,
        COLOR_PRO_BADGE_BORDER,
        1,
    );
    SelectObject(mem_dc, font_pro_pill as *mut _);
    SetTextColor(mem_dc, COLOR_PRO_BADGE_TEXT);
    let pro_txt = to_wstring("✦ PRO");
    let mut pro_rc = RECT {
        left: pro_x,
        top: pro_y,
        right: pro_x + pro_w,
        bottom: pro_y + pro_h,
    };
    DrawTextW(
        mem_dc,
        pro_txt.as_ptr(),
        -1,
        &mut pro_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // Window Controls (Minimize & Close Pills)
    let btn_w = 36;
    let btn_h = 28;
    let btn_y = (TOPBAR_H - btn_h) / 2;
    let close_x = width - CARD_X - btn_w;
    let min_x = close_x - btn_w - 6;

    let is_close_hover = hov == Some(HitArea::BtnClose);
    let is_min_hover = hov == Some(HitArea::BtnMinimize);

    if is_min_hover {
        draw_round_rect(
            mem_dc,
            min_x,
            btn_y,
            min_x + btn_w,
            btn_y + btn_h,
            8,
            rgb(26, 22, 30),
            rgb(60, 45, 55),
            1,
        );
    }
    SelectObject(mem_dc, font_icon_window as *mut _);
    SetTextColor(
        mem_dc,
        if is_min_hover {
            COLOR_TEXT_WHITE
        } else {
            COLOR_TEXT_SECONDARY
        },
    );
    let min_w = to_wstring(GLYPH_MINIMIZE);
    let mut min_rect = RECT {
        left: min_x,
        top: btn_y,
        right: min_x + btn_w,
        bottom: btn_y + btn_h,
    };
    DrawTextW(
        mem_dc,
        min_w.as_ptr(),
        -1,
        &mut min_rect,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    if is_close_hover {
        draw_round_rect(
            mem_dc,
            close_x,
            btn_y,
            close_x + btn_w,
            btn_y + btn_h,
            8,
            COLOR_RIOT_RED,
            COLOR_PRO_RED,
            1,
        );
    }
    SelectObject(mem_dc, font_icon_window as *mut _);
    SetTextColor(
        mem_dc,
        if is_close_hover {
            COLOR_TEXT_WHITE
        } else {
            COLOR_TEXT_SECONDARY
        },
    );
    let close_w = to_wstring(GLYPH_CLOSE);
    let mut close_rect = RECT {
        left: close_x,
        top: btn_y,
        right: close_x + btn_w,
        bottom: btn_y + btn_h,
    };
    DrawTextW(
        mem_dc,
        close_w.as_ptr(),
        -1,
        &mut close_rect,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // =========================================================================
    // 4. WORKSPACE HEADER & MANUAL SYNC BUTTON (52..132px)
    // =========================================================================
    SelectObject(mem_dc, font_header as *mut _);
    SetTextColor(mem_dc, COLOR_TEXT_WHITE);
    let title_w = to_wstring("Account Workspace");
    let mut title_rect = RECT {
        left: CARD_X,
        top: 64,
        right: width - 120,
        bottom: 94,
    };
    DrawTextW(
        mem_dc,
        title_w.as_ptr(),
        -1,
        &mut title_rect,
        (DT_LEFT | DT_SINGLELINE) as u32,
    );

    SelectObject(mem_dc, font_sub as *mut _);
    SetTextColor(mem_dc, COLOR_TEXT_SECONDARY);
    let sub_w = to_wstring("Instant 1-click switch • Rotating token protection");
    let mut sub_rect = RECT {
        left: CARD_X,
        top: 96,
        right: width - 120,
        bottom: 118,
    };
    DrawTextW(
        mem_dc,
        sub_w.as_ptr(),
        -1,
        &mut sub_rect,
        (DT_LEFT | DT_SINGLELINE) as u32,
    );

    // "↻ Sync" Pill Button on Header Right
    let sync_w = 82;
    let sync_h = 32;
    let sync_x = width - CARD_X - sync_w;
    let sync_y = 72;
    let is_sync_hover = hov == Some(HitArea::BtnSync);
    draw_round_rect(
        mem_dc,
        sync_x,
        sync_y,
        sync_x + sync_w,
        sync_y + sync_h,
        10,
        if is_sync_hover {
            rgb(34, 12, 18)
        } else {
            rgb(14, 11, 16)
        },
        if is_sync_hover {
            COLOR_PRO_RED
        } else {
            rgb(58, 20, 30)
        },
        1,
    );
    SelectObject(mem_dc, font_meta as *mut _);
    SetTextColor(
        mem_dc,
        if is_sync_hover {
            COLOR_TEXT_WHITE
        } else {
            COLOR_PRO_BADGE_TEXT
        },
    );
    let sync_txt = to_wstring("↻  Sync");
    let mut sync_rc = RECT {
        left: sync_x,
        top: sync_y,
        right: sync_x + sync_w,
        bottom: sync_y + sync_h,
    };
    DrawTextW(
        mem_dc,
        sync_txt.as_ptr(),
        -1,
        &mut sync_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // =========================================================================
    // 5. SCROLLABLE ACCOUNT CARDS VIEWPORT (132..480px)
    // =========================================================================
    let card_w = width - (CARD_X * 2);
    let list_start_y = HEADER_END_Y;

    let active_game_name = active_acc
        .as_ref()
        .map(|a| a.game_name.to_lowercase())
        .unwrap_or_default();
    let active_tag_line = active_acc
        .as_ref()
        .map(|a| a.tag_line.to_lowercase())
        .unwrap_or_default();

    let clip_rgn = CreateRectRgn(0, list_start_y - 2, width, DOCK_START_Y - 2);
    SelectClipRgn(mem_dc, clip_rgn);

    if config.accounts.is_empty() {
        let box_y = list_start_y + 12;
        let box_h = 220;
        draw_round_rect(
            mem_dc,
            CARD_X,
            box_y,
            CARD_X + card_w,
            box_y + box_h,
            14,
            COLOR_CARD_BG,
            COLOR_CARD_BORDER_ACTIVE,
            1,
        );

        SelectObject(mem_dc, font_header as *mut _);
        SetTextColor(mem_dc, COLOR_PRO_RED);
        let spark_w = to_wstring("✦");
        let mut spark_rc = RECT {
            left: CARD_X,
            top: box_y + 24,
            right: CARD_X + card_w,
            bottom: box_y + 58,
        };
        DrawTextW(
            mem_dc,
            spark_w.as_ptr(),
            -1,
            &mut spark_rc,
            (DT_CENTER | DT_SINGLELINE) as u32,
        );

        SelectObject(mem_dc, font_name_large as *mut _);
        SetTextColor(mem_dc, COLOR_TEXT_WHITE);
        let msg_title = to_wstring("No Riot Accounts Linked Yet");
        let mut t_rc = RECT {
            left: CARD_X,
            top: box_y + 66,
            right: CARD_X + card_w,
            bottom: box_y + 94,
        };
        DrawTextW(
            mem_dc,
            msg_title.as_ptr(),
            -1,
            &mut t_rc,
            (DT_CENTER | DT_SINGLELINE) as u32,
        );

        SelectObject(mem_dc, font_sub as *mut _);
        SetTextColor(mem_dc, COLOR_TEXT_SECONDARY);
        let msg_desc = to_wstring(
            "1. Click '+ Add / Switch to New Riot Account' below.\n2. Sign into Riot Client with 'Stay signed in' checked.\n3. Your session is captured & protected automatically!",
        );
        let mut d_rc = RECT {
            left: CARD_X + 24,
            top: box_y + 108,
            right: CARD_X + card_w - 24,
            bottom: box_y + 195,
        };
        DrawTextW(
            mem_dc,
            msg_desc.as_ptr(),
            -1,
            &mut d_rc,
            (DT_CENTER | DT_WORDBREAK) as u32,
        );
    } else {
        for i in 0..config.accounts.len() {
            let acc = &config.accounts[i];
            let item_y = list_start_y - scroll_offset + (i as i32 * (CARD_H + CARD_SPACING));
            let card_cy = item_y + (CARD_H / 2);

            if item_y + CARD_H < list_start_y || item_y > DOCK_START_Y {
                continue;
            }

            let is_current = !active_game_name.is_empty()
                && acc.game_name.to_lowercase() == active_game_name
                && acc.tag_line.to_lowercase() == active_tag_line;

            let is_card_hover = hov == Some(HitArea::Account(i));
            let is_play_hover = hov == Some(HitArea::PlayBtn(i));
            let is_del_hover = hov == Some(HitArea::Delete(i));
            let any_hover = is_card_hover || is_play_hover || is_del_hover;

            let bg_color = if any_hover {
                COLOR_CARD_HOVER
            } else if is_current {
                COLOR_CARD_ACTIVE
            } else {
                COLOR_CARD_BG
            };

            let border_color = if any_hover {
                COLOR_CARD_BORDER_HOVER
            } else if is_current {
                COLOR_CARD_BORDER_ACTIVE
            } else {
                COLOR_CARD_BORDER
            };

            // Card Container
            draw_round_rect(
                mem_dc,
                CARD_X,
                item_y,
                CARD_X + card_w,
                item_y + CARD_H,
                14,
                bg_color,
                border_color,
                if any_hover { 2 } else { 1 },
            );

            // Google AI Pro Left Accent Glow Pill (for active or hovered card)
            if is_current || any_hover {
                let bar_col = if any_hover {
                    COLOR_PRO_RED
                } else {
                    COLOR_RIOT_RED
                };
                draw_round_rect(
                    mem_dc,
                    CARD_X + 4,
                    item_y + 16,
                    CARD_X + 8,
                    item_y + CARD_H - 16,
                    4,
                    bar_col,
                    bar_col,
                    0,
                );
            }

            // =========================================================================
            // SUMMONER AVATAR OR CRIMSON MONOGRAM
            // =========================================================================
            let av_size = 50;
            let av_x = CARD_X + 16;
            let av_y = card_cy - (av_size / 2);

            let (acc_icon, has_custom_ico): (HICON, bool) = {
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        let ico_file = state.icons_dir.join(format!("{}.ico", acc.id));
                        let exists = ico_file.exists();
                        if let Some(&icon) = state.account_icons.get(&acc.id) {
                            (icon, exists)
                        } else {
                            let mut loaded_icon = 0 as HICON;
                            if exists {
                                let path_w = to_wstring(ico_file.to_str().unwrap_or_default());
                                let h = LoadImageW(
                                    0 as HINSTANCE,
                                    path_w.as_ptr(),
                                    IMAGE_ICON_TYPE,
                                    46,
                                    46,
                                    LR_LOADFROMFILE_FLAG,
                                );
                                if h != 0 as HANDLE {
                                    loaded_icon = h as HICON;
                                }
                            }
                            if loaded_icon == 0 as HICON {
                                loaded_icon = state.official_icon;
                            }
                            state.account_icons.insert(acc.id.clone(), loaded_icon);
                            (loaded_icon, exists)
                        }
                    } else {
                        (0 as HICON, false)
                    }
                } else {
                    (0 as HICON, false)
                }
            };

            draw_round_rect(
                mem_dc,
                av_x,
                av_y,
                av_x + av_size,
                av_y + av_size,
                12,
                rgb(18, 8, 12),
                if is_current || any_hover {
                    COLOR_PRO_RED
                } else {
                    rgb(48, 22, 30)
                },
                if is_current { 2 } else { 1 },
            );

            if has_custom_ico && acc_icon != 0 as HICON {
                DrawIconEx(
                    mem_dc,
                    av_x + 2,
                    av_y + 2,
                    acc_icon,
                    46,
                    46,
                    0,
                    0 as HBRUSH,
                    DI_NORMAL_FLAG,
                );
            } else {
                // Google AI Pro Monogram Avatar
                SelectObject(mem_dc, font_header as *mut _);
                SetTextColor(mem_dc, COLOR_PRO_RED);
                let first_ch = acc
                    .game_name
                    .chars()
                    .next()
                    .unwrap_or('R')
                    .to_uppercase()
                    .to_string();
                let ch_w = to_wstring(&first_ch);
                let mut av_rc = RECT {
                    left: av_x,
                    top: av_y,
                    right: av_x + av_size,
                    bottom: av_y + av_size,
                };
                DrawTextW(
                    mem_dc,
                    ch_w.as_ptr(),
                    -1,
                    &mut av_rc,
                    (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
                );
            }

            // =========================================================================
            // RIGHT SIDE CONTROLS & 2X SERVER BADGE (COMPUTED FIRST TO PREVENT OVERLAP)
            // =========================================================================
            let play_cx = CARD_X + card_w - 30;
            let trash_cx = play_cx - 42;

            draw_pro_play_button(
                mem_dc,
                play_cx,
                card_cy,
                18,
                is_play_hover || is_card_hover,
                is_current,
                font_icon_play,
            );
            draw_pro_trash_button(mem_dc, trash_cx, card_cy, 30, is_del_hover, font_icon_trash);

            // 2x Server Region Badge (Right-aligned to the left of Trash button)
            let server_str = format!(" {} ", acc.region);
            let server_w = to_wstring(&server_str);
            let mut server_sz: SIZE = std::mem::zeroed();
            SelectObject(mem_dc, font_server_badge as *mut _);
            GetTextExtentPoint32W(
                mem_dc,
                server_w.as_ptr(),
                server_w.len() as i32 - 1,
                &mut server_sz,
            );

            let s_badge_w = (server_sz.cx + 10).max(54);
            let s_badge_h = 32;
            let badge_x = trash_cx - 22 - s_badge_w;
            let s_badge_y = card_cy - (s_badge_h / 2);

            draw_round_rect(
                mem_dc,
                badge_x,
                s_badge_y,
                badge_x + s_badge_w,
                s_badge_y + s_badge_h,
                8,
                if any_hover {
                    rgb(42, 10, 18)
                } else {
                    COLOR_SERVER_BG
                },
                if any_hover {
                    COLOR_PRO_RED
                } else {
                    COLOR_SERVER_BORDER
                },
                1,
            );

            SetTextColor(
                mem_dc,
                if any_hover {
                    COLOR_TEXT_WHITE
                } else {
                    COLOR_SERVER_TEXT
                },
            );
            let mut server_rect = RECT {
                left: badge_x,
                top: s_badge_y,
                right: badge_x + s_badge_w,
                bottom: s_badge_y + s_badge_h,
            };
            DrawTextW(
                mem_dc,
                server_w.as_ptr(),
                -1,
                &mut server_rect,
                (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
            );

            // =========================================================================
            // TWO-ROW ACCOUNT TYPOGRAPHY (CLIPPED BEFORE SERVER BADGE)
            // =========================================================================
            let text_x = CARD_X + 78;
            let max_text_right = badge_x - 10;

            // Row 1: GameName + #TagLine
            SelectObject(mem_dc, font_name_large as *mut _);
            SetTextColor(mem_dc, COLOR_TEXT_WHITE);
            let name_w = to_wstring(&acc.game_name);
            let mut name_sz: SIZE = std::mem::zeroed();
            GetTextExtentPoint32W(
                mem_dc,
                name_w.as_ptr(),
                name_w.len() as i32 - 1,
                &mut name_sz,
            );

            let row1_top = item_y + 15;
            let row1_bottom = row1_top + 26;
            let name_right = (text_x + name_sz.cx).min(max_text_right - 36);
            let mut name_rect = RECT {
                left: text_x,
                top: row1_top,
                right: name_right,
                bottom: row1_bottom,
            };
            DrawTextW(
                mem_dc,
                name_w.as_ptr(),
                -1,
                &mut name_rect,
                (DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS) as u32,
            );

            if !acc.tag_line.is_empty() && name_right + 8 < max_text_right {
                SelectObject(mem_dc, font_tag as *mut _);
                SetTextColor(mem_dc, COLOR_TEXT_SECONDARY);
                let tag_str = format!("#{}", acc.tag_line);
                let tag_w = to_wstring(&tag_str);
                let mut tag_rect = RECT {
                    left: name_right + 6,
                    top: row1_top + 2,
                    right: max_text_right,
                    bottom: row1_bottom,
                };
                DrawTextW(
                    mem_dc,
                    tag_w.as_ptr(),
                    -1,
                    &mut tag_rect,
                    (DT_LEFT | DT_VCENTER | DT_SINGLELINE) as u32,
                );
            }

            // Row 2: Status Dot + Session State Label
            let row2_top = item_y + 44;
            let row2_bottom = row2_top + 20;
            let dot_cx = text_x + 5;
            let dot_cy = row2_top + 10;

            if is_current {
                draw_status_dot(mem_dc, dot_cx, dot_cy, COLOR_GOOGLE_GREEN, rgb(14, 52, 24));
                SelectObject(mem_dc, font_meta as *mut _);
                SetTextColor(mem_dc, COLOR_GOOGLE_GREEN);
                let status_label = if acc.username.is_empty() {
                    "ACTIVE SESSION".to_string()
                } else {
                    format!("ACTIVE • {}", acc.username)
                };
                let st_w = to_wstring(&status_label);
                let mut st_rc = RECT {
                    left: text_x + 15,
                    top: row2_top,
                    right: max_text_right,
                    bottom: row2_bottom,
                };
                DrawTextW(
                    mem_dc,
                    st_w.as_ptr(),
                    -1,
                    &mut st_rc,
                    (DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS) as u32,
                );
            } else {
                draw_status_dot(
                    mem_dc,
                    dot_cx,
                    dot_cy,
                    if any_hover {
                        COLOR_PRO_RED
                    } else {
                        rgb(120, 35, 50)
                    },
                    rgb(32, 10, 16),
                );
                SelectObject(mem_dc, font_meta as *mut _);
                SetTextColor(
                    mem_dc,
                    if any_hover {
                        COLOR_PRO_BADGE_TEXT
                    } else {
                        COLOR_TEXT_MUTED
                    },
                );
                let status_label = if acc.username.is_empty() {
                    "READY TO SWITCH".to_string()
                } else {
                    format!("READY • {}", acc.username)
                };
                let st_w = to_wstring(&status_label);
                let mut st_rc = RECT {
                    left: text_x + 15,
                    top: row2_top,
                    right: max_text_right,
                    bottom: row2_bottom,
                };
                DrawTextW(
                    mem_dc,
                    st_w.as_ptr(),
                    -1,
                    &mut st_rc,
                    (DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS) as u32,
                );
            }
        }

        // Scrollbar Thumb Indicator if accounts exceed viewport
        let total_content_h =
            (config.accounts.len() as i32) * (CARD_H + CARD_SPACING) - CARD_SPACING;
        let visible_h = DOCK_START_Y - list_start_y - 12;
        if total_content_h > visible_h {
            let track_h = visible_h;
            let thumb_h = ((visible_h * visible_h) / total_content_h).max(28);
            let max_scroll = (total_content_h - visible_h).max(1);
            let thumb_y =
                list_start_y + ((scroll_offset * (track_h - thumb_h)) / max_scroll);
            draw_round_rect(
                mem_dc,
                width - 8,
                thumb_y,
                width - 4,
                thumb_y + thumb_h,
                2,
                COLOR_CARD_BORDER_ACTIVE,
                COLOR_CARD_BORDER_ACTIVE,
                0,
            );
        }
    }

    SelectClipRgn(mem_dc, 0 as HRGN);
    DeleteObject(clip_rgn as *mut _);

    // =========================================================================
    // 6. FIXED GOOGLE AI PRO BOTTOM DOCK (480..580px)
    // =========================================================================
    let add_btn_y = DOCK_START_Y + 12;
    let add_btn_h = 44;
    let is_add_hover = hov == Some(HitArea::AddAccount);

    let add_bg = if is_add_hover {
        rgb(38, 10, 18)
    } else {
        rgb(18, 8, 12)
    };
    let add_border = if is_add_hover {
        COLOR_PRO_RED
    } else {
        rgb(100, 20, 38)
    };

    draw_round_rect(
        mem_dc,
        CARD_X,
        add_btn_y,
        CARD_X + card_w,
        add_btn_y + add_btn_h,
        12,
        add_bg,
        add_border,
        if is_add_hover { 2 } else { 1 },
    );

    SelectObject(mem_dc, font_btn_bold as *mut _);
    SetTextColor(
        mem_dc,
        if is_add_hover {
            COLOR_TEXT_WHITE
        } else {
            COLOR_PRO_BADGE_TEXT
        },
    );
    let add_text_w = to_wstring("+   Add / Switch to New Riot Account");
    let mut add_rc = RECT {
        left: CARD_X,
        top: add_btn_y,
        right: CARD_X + card_w,
        bottom: add_btn_y + add_btn_h,
    };
    DrawTextW(
        mem_dc,
        add_text_w.as_ptr(),
        -1,
        &mut add_rc,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32,
    );

    // Status Footer Bar
    let status_y = height - 28;
    SelectObject(mem_dc, font_meta as *mut _);
    let status_color = if is_err {
        COLOR_PRO_RED
    } else if is_busy {
        COLOR_PRO_BADGE_TEXT
    } else {
        COLOR_TEXT_SECONDARY
    };
    SetTextColor(mem_dc, status_color);
    let fb_w = to_wstring(&feedback);
    let mut fb_rect = RECT {
        left: CARD_X,
        top: status_y,
        right: width - CARD_X,
        bottom: status_y + 20,
    };
    DrawTextW(
        mem_dc,
        fb_w.as_ptr(),
        -1,
        &mut fb_rect,
        (DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS) as u32,
    );

    // Blit to screen
    BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

    // Clean up GDI objects
    SelectObject(mem_dc, old_bmp);
    DeleteObject(mem_bmp as *mut _);
    DeleteDC(mem_dc);

    DeleteObject(font_brand as *mut _);
    DeleteObject(font_pro_pill as *mut _);
    DeleteObject(font_header as *mut _);
    DeleteObject(font_sub as *mut _);
    DeleteObject(font_btn_bold as *mut _);
    DeleteObject(font_name_large as *mut _);
    DeleteObject(font_tag as *mut _);
    DeleteObject(font_server_badge as *mut _);
    DeleteObject(font_meta as *mut _);
    DeleteObject(font_icon_play as *mut _);
    DeleteObject(font_icon_trash as *mut _);
    DeleteObject(font_icon_window as *mut _);
}

// =========================================================================
// MOUSE & INTERACTION HANDLING
// =========================================================================

unsafe fn handle_mouse_move(hwnd: HWND, x: i32, y: i32) {
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    let width = rect.right - rect.left;

    // Enable WM_MOUSELEAVE tracking so hover state clears when cursor exits window
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

    let btn_w = 36;
    let btn_h = 28;
    let btn_y = (TOPBAR_H - btn_h) / 2;
    let close_x = width - CARD_X - btn_w;
    let min_x = close_x - btn_w - 6;

    let sync_w = 82;
    let sync_h = 32;
    let sync_x = width - CARD_X - sync_w;
    let sync_y = 72;

    let card_w = width - (CARD_X * 2);
    let add_btn_y = DOCK_START_Y + 12;
    let add_btn_h = 44;

    let mut new_hit: Option<HitArea> = None;

    if y >= 0 && y <= TOPBAR_H {
        if x >= close_x && x <= close_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
            new_hit = Some(HitArea::BtnClose);
        } else if x >= min_x && x <= min_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
            new_hit = Some(HitArea::BtnMinimize);
        } else {
            new_hit = Some(HitArea::TitleBar);
        }
    } else if x >= sync_x && x <= sync_x + sync_w && y >= sync_y && y <= sync_y + sync_h {
        new_hit = Some(HitArea::BtnSync);
    } else if x >= CARD_X && x <= CARD_X + card_w && y >= add_btn_y && y <= add_btn_y + add_btn_h {
        new_hit = Some(HitArea::AddAccount);
    } else if y >= HEADER_END_Y && y < DOCK_START_Y {
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

        for i in 0..acc_count {
            let item_y = HEADER_END_Y - scroll_offset + (i as i32 * (CARD_H + CARD_SPACING));
            if x >= CARD_X && x <= CARD_X + card_w && y >= item_y && y <= item_y + CARD_H {
                let card_cy = item_y + (CARD_H / 2);
                let play_cx = CARD_X + card_w - 30;
                let trash_cx = play_cx - 42;

                if (x - play_cx).pow(2) + (y - card_cy).pow(2) <= 20 * 20 {
                    new_hit = Some(HitArea::PlayBtn(i));
                } else if (x - trash_cx).abs() <= 16 && (y - card_cy).abs() <= 16 {
                    new_hit = Some(HitArea::Delete(i));
                } else {
                    new_hit = Some(HitArea::Account(i));
                }
                break;
            }
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
            HitArea::BtnMinimize => {
                hide_to_tray(hwnd);
            }
            HitArea::BtnClose => {
                DestroyWindow(hwnd);
            }
            HitArea::BtnSync => {
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
                                    state.feedback_text = format!(
                                        "✦ Synced active session: {} #{}",
                                        p.game_name, p.tag_line
                                    );
                                    state.feedback_is_error = false;
                                }
                                Err(e) => {
                                    state.feedback_text = e;
                                    state.feedback_is_error = true;
                                }
                            }
                        } else {
                            state.feedback_text =
                                "No active persistent session found in Riot Client.".to_string();
                            state.feedback_is_error = true;
                        }
                    }
                }
                InvalidateRect(hwnd, null_mut(), 0);
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
                                "⚡ Backing up active session & preparing clean login..."
                                    .to_string();
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
                    // Layer 1: Backup current active account before clearing!
                    storage.backup_current_active_session(&riot);
                    riot.prepare_clean_login();
                    let _ = riot.launch("riot_client");

                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(mut state) = state_arc.lock() {
                            state.is_busy = false;
                            state.active_account = None;
                            state.feedback_text =
                                "✦ Riot Client ready! Sign in with 'Stay signed in' checked."
                                    .to_string();
                            state.feedback_is_error = false;
                        }
                    }
                    unsafe {
                        PostMessageW(hwnd_val as HWND, WM_APP_ASYNC_DONE, 0, 0);
                    }
                });
            }
            HitArea::Account(idx) | HitArea::PlayBtn(idx) => {
                if is_busy {
                    return;
                }
                let (target_id, target_name, riot, storage) = {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(mut state) = state_arc.lock() {
                            let conf = state.storage.load_config();
                            if idx < conf.accounts.len() {
                                let id = conf.accounts[idx].id.clone();
                                let name = format!(
                                    "{} #{}",
                                    conf.accounts[idx].game_name, conf.accounts[idx].tag_line
                                );
                                state.is_busy = true;
                                state.feedback_text =
                                    format!("⚡ Switching session to {}...", name);
                                state.feedback_is_error = false;
                                (id, name, state.riot.clone(), state.storage.clone())
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
                    // 1. Pre-switch backup of currently active account's rotated token
                    storage.backup_current_active_session(&riot);

                    // 2. Kill all Riot Client & game processes cleanly
                    RiotManager::kill_riot_processes();

                    // 3. Post-kill backup in case Riot Client flushed token on exit
                    storage.backup_current_active_session(&riot);

                    // 4. Apply target profile session
                    let res = storage.apply_profile(&target_id, &riot.settings_yaml_path);

                    match res {
                        Ok(_) => {
                            let launch_res = riot.launch("riot_client");
                            if let Some(state_arc) = GUI_STATE.get() {
                                if let Ok(mut state) = state_arc.lock() {
                                    state.is_busy = false;
                                    state.active_account = state.riot.get_active_account();
                                    match launch_res {
                                        Ok(_) => {
                                            state.feedback_text =
                                                format!("✦ Launched: {}", target_name);
                                            state.feedback_is_error = false;
                                        }
                                        Err(e) => {
                                            state.feedback_text = format!("Error: {}", e);
                                            state.feedback_is_error = true;
                                        }
                                    }
                                }
                            }
                            unsafe {
                                PostMessageW(hwnd_val as HWND, WM_APP_ASYNC_DONE, 0, 0);
                            }

                            // Layer 2: Capture rotated refresh token at +4s and +8s after Riot Client boot
                            for delay_secs in [4u64, 4u64] {
                                std::thread::sleep(std::time::Duration::from_secs(delay_secs));
                                storage.backup_current_active_session(&riot);
                                unsafe {
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
                            unsafe {
                                PostMessageW(hwnd_val as HWND, WM_APP_ASYNC_DONE, 0, 0);
                            }
                        }
                    }
                });
            }
            HitArea::Delete(idx) => {
                if is_busy {
                    return;
                }
                let target_id = {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(state) = state_arc.lock() {
                            let conf = state.storage.load_config();
                            if idx < conf.accounts.len() {
                                conf.accounts[idx].id.clone()
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

                let confirm_txt = to_wstring("Remove this account from Riot Fast Login?");
                let confirm_title = to_wstring("Confirm Remove");
                let confirm = MessageBoxW(
                    hwnd,
                    confirm_txt.as_ptr(),
                    confirm_title.as_ptr(),
                    MB_YESNO | MB_ICONQUESTION,
                );
                if confirm == IDYES {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(mut state) = state_arc.lock() {
                            let _ = state.storage.delete_profile(&target_id);
                            state.account_icons.remove(&target_id);
                            state.feedback_text = "Account removed from workspace.".to_string();
                            state.feedback_is_error = false;
                        }
                    }
                    InvalidateRect(hwnd, null_mut(), 0);
                }
            }
        }
    }
}
