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
pub const WM_TRAYICON: u32        = WM_USER + 100;
const ID_TRAY_OPEN: usize        = 2001;
const ID_TRAY_EXIT: usize        = 2002;

// ==========================================
// iOS DARK & RIOT FUSION COLOR PALETTE (BGR)
// ==========================================
const COLOR_WINDOW_BG: u32       = 0x00101010; // #101010 iOS Deep Dark / Riot Charcoal
const COLOR_OUTER_BORDER: u32    = 0x00282420; // #202428 Subtle Window Border
const COLOR_CARD_BG: u32         = 0x001C1A18; // #181A1C iOS Grouped Card
const COLOR_CARD_HOVER: u32      = 0x002A2622; // #22262A Hover Surface
const COLOR_CARD_BORDER: u32     = 0x002D2824; // #24282D Card Border
const COLOR_CARD_ACTIVE: u32     = 0x00221E16; // #161E22 Active Selected Card

// iOS & Riot Accents
const COLOR_RIOT_RED: u32        = 0x002900EB; // #EB0029 Riot Red
const COLOR_RIOT_RED_LIGHT: u32  = 0x004520FF; // Glowing Red Ring
const COLOR_RIOT_GOLD: u32       = 0x006EAAC8; // #C8AA6E Hextech Gold
const COLOR_IOS_GREEN: u32       = 0x0058D130; // #30D158 Official Apple iOS Green

const COLOR_TEXT_WHITE: u32      = 0x00FFFFFF; // Crisp Pure White
const COLOR_TEXT_GRAY: u32       = 0x008E8E93; // #938E8E iOS Secondary Label
const COLOR_TEXT_MUTED: u32      = 0x00504A44; // #444A50 Dark Muted Gray
const COLOR_CLOSE_HOVER: u32     = 0x002900EB; // Red on close button hover

const TIMER_AUTODETECT: usize    = 1001;

// Win32 Icon constants
const IMAGE_ICON_TYPE: u32       = 1;
const LR_LOADFROMFILE_FLAG: u32  = 0x00000010;
const DI_NORMAL_FLAG: u32        = 0x0003;

// Authentic Riot Games Official Typography
const FONT_BEAUFORT_BOLD: &[u8]   = include_bytes!("../assets/fonts/BeaufortforLOL-Bold.ttf");
const FONT_BEAUFORT_MEDIUM: &[u8] = include_bytes!("../assets/fonts/BeaufortforLOL-Medium.ttf");

// Unicode Vector Glyphs (from Segoe Fluent Icons / Segoe MDL2 Assets)
const GLYPH_PLAY: &str           = "\u{E768}"; // Official modern Play glyph
const GLYPH_TRASH: &str          = "\u{E74D}"; // Official modern Trashcan glyph
const GLYPH_MINIMIZE: &str       = "\u{E921}"; // Fluent Minimize
const GLYPH_CLOSE: &str          = "\u{E8BB}"; // Fluent Close (✕)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitArea {
    TitleBar,
    BtnClose,
    BtnMinimize,
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
    // Try resource index 0 (rcedit primary icon) then 1
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

/// Automatically detects any newly logged-in Riot account and auto-saves it silently
fn check_and_autosave(state: &mut GuiState) -> bool {
    let current_active = state.riot.get_active_account();
    let had_change = match (&state.active_account, &current_active) {
        (Some(old), Some(new)) => old.game_name != new.game_name || old.tag_line != new.tag_line,
        (None, Some(_)) => true,
        (Some(_), None) => true,
        (None, None) => false,
    };

    state.active_account = current_active.clone();

    if let Some(ref act) = current_active {
        if !act.game_name.is_empty() {
            // ALWAYS update the saved YAML profile with the latest active session tokens!
            let _ = state.storage.update_saved_account_session(act, &state.riot.settings_yaml_path);

            let conf = state.storage.load_config();
            let already_saved = conf.accounts.iter().any(|a| {
                a.game_name.eq_ignore_ascii_case(&act.game_name)
                    && a.tag_line.eq_ignore_ascii_case(&act.tag_line)
            });

            if !already_saved {
                if let Ok(p) = state.storage.save_active_account(act, None, &state.riot.settings_yaml_path) {
                    state.feedback_text = format!("Linked new account: {}#{}", p.game_name, p.tag_line);
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
        // Register authentic Riot Games fonts into memory for this process
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
        let official_riot_icon = load_official_riot_icon_sized(48);

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
        let win_w = 440;
        let win_h = 520;
        let start_x = (screen_w - win_w) / 2;
        let start_y = (screen_h - win_h) / 2;

        let window_title = to_wstring("Riot Fast Login");
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            start_x,
            start_y,
            win_w,
            win_h,
            0 as HWND,
            0 as HMENU,
            hinstance,
            null_mut(),
        );

        if hwnd == 0 as HWND {
            return;
        }

        // Set high-quality window & taskbar icons (dark black icon)
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as WPARAM, app_icon_big as LPARAM);
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as WPARAM, app_icon_small as LPARAM);

        // Add System Tray Icon (Taskbar notification area)
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = app_icon_small;
        let tip = to_wstring("Riot Fast Login");
        let tip_len = tip.len().min(nid.szTip.len() - 1);
        nid.szTip[..tip_len].copy_from_slice(&tip[..tip_len]);
        Shell_NotifyIconW(NIM_ADD, &nid);

        let dark_mode: i32 = 1;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            &dark_mode as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
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
            feedback_text: "Select an account to launch".to_string(),
            feedback_is_error: false,
            official_icon: official_riot_icon,
            app_icon_small,
            account_icons: HashMap::new(),
            icons_dir,
            scroll_offset: 0,
        };

        // Auto-detect on launch
        check_and_autosave(&mut state);

        let _ = GUI_STATE.set(Arc::new(Mutex::new(state)));

        // Background poller every 2s
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
                    let open_str = to_wstring("Open Riot Fast Login");
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
                    let total_content_h = (acc_count as i32) * (76 + 10);
                    let visible_h = 520 - (65 + 64 + 35);
                    let max_scroll = (total_content_h - visible_h).max(0);

                    let step = (delta / 120) * 45;
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

/// Official Riot Games Display Font (Beaufort for LOL) with clean fallback
unsafe fn create_riot_font(size: i32, bold: bool) -> HFONT {
    let font = create_font("Beaufort for LOL", size, bold);
    if font != 0 as HFONT {
        font
    } else {
        create_font("Segoe UI", size, bold)
    }
}

/// Official Riot Games UI / Subtitle Font (Beaufort for LOL Medium) with clean fallback
unsafe fn create_riot_sub_font(size: i32, bold: bool) -> HFONT {
    let font = create_font("Beaufort for LOL Medium", size, bold);
    if font != 0 as HFONT {
        font
    } else {
        create_font("Segoe UI", size, bold)
    }
}

/// Creates vector icon font with anti-aliasing
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

// =========================================================================
// HIGH-END iOS STYLE VECTOR CONTROLS
// =========================================================================

/// 1. iOS Style Play Button (Glassmorphism Circular Action Button + Smooth Vector Glyph)
unsafe fn draw_ios_play_button(hdc: HDC, cx: i32, cy: i32, radius: i32, is_hover: bool, icon_font: HFONT) {
    let bg_col = if is_hover { COLOR_RIOT_RED } else { 0x00262420 };
    let border_col = if is_hover { COLOR_RIOT_RED_LIGHT } else { 0x003A342C };

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
    SetTextColor(hdc, COLOR_TEXT_WHITE);
    let play_w = to_wstring(GLYPH_PLAY);
    let mut play_rect = RECT {
        left: cx - radius + 1,
        top: cy - radius,
        right: cx + radius + 1,
        bottom: cy + radius,
    };
    DrawTextW(hdc, play_w.as_ptr(), -1, &mut play_rect, (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32);
}

/// 2. iOS Style Trash Button (Minimalist Rounded SF Symbol Style + Vector Glyph)
unsafe fn draw_ios_trash_button(hdc: HDC, cx: i32, cy: i32, size: i32, is_hover: bool, icon_font: HFONT) {
    let half = size / 2;
    let bg_col = if is_hover { 0x00281214 } else { 0x001E1A18 };
    let border_col = if is_hover { COLOR_RIOT_RED } else { 0x002E2824 };

    draw_round_rect(hdc, cx - half, cy - half, cx + half, cy + half, 8, bg_col, border_col, 1);

    SelectObject(hdc, icon_font as *mut _);
    SetTextColor(hdc, if is_hover { COLOR_RIOT_RED } else { COLOR_TEXT_MUTED });
    let trash_w = to_wstring(GLYPH_TRASH);
    let mut trash_rect = RECT {
        left: cx - half,
        top: cy - half,
        right: cx + half,
        bottom: cy + half,
    };
    DrawTextW(hdc, trash_w.as_ptr(), -1, &mut trash_rect, (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32);
}

/// 3. Modern iOS Active Indicator (Apple Green FaceTime / Dynamic Island LED Dot)
unsafe fn draw_ios_active_indicator(hdc: HDC, cx: i32, cy: i32) {
    let halo_brush = CreateSolidBrush(0x001B4210);
    let null_pen = CreatePen(PS_NULL as i32, 0, 0);
    let old_b = SelectObject(hdc, halo_brush as *mut _);
    let old_p = SelectObject(hdc, null_pen as *mut _);
    Ellipse(hdc, cx - 8, cy - 8, cx + 9, cy + 9);

    let mid_brush = CreateSolidBrush(0x00256818);
    SelectObject(hdc, mid_brush as *mut _);
    Ellipse(hdc, cx - 5, cy - 5, cx + 6, cy + 6);

    let core_brush = CreateSolidBrush(COLOR_IOS_GREEN);
    SelectObject(hdc, core_brush as *mut _);
    Ellipse(hdc, cx - 3, cy - 3, cx + 4, cy + 4);

    SelectObject(hdc, old_b);
    SelectObject(hdc, old_p);
    DeleteObject(halo_brush as *mut _);
    DeleteObject(mid_brush as *mut _);
    DeleteObject(core_brush as *mut _);
    DeleteObject(null_pen as *mut _);
}

// =========================================================================
// MAIN RENDER LOOP
// =========================================================================

unsafe fn render_riot_client(hwnd: HWND, hdc: HDC) {
    let mut rect: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    let mem_dc = CreateCompatibleDC(hdc);
    let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
    let old_bmp = SelectObject(mem_dc, mem_bmp as *mut _);

    // 1. Fill Background
    let bg_brush = CreateSolidBrush(COLOR_WINDOW_BG);
    FillRect(mem_dc, &rect, bg_brush as *mut _);
    DeleteObject(bg_brush as *mut _);

    // 2. Outer Window Border
    let border_pen = CreatePen(PS_SOLID as i32, 1, COLOR_OUTER_BORDER);
    let old_p = SelectObject(mem_dc, border_pen as *mut _);
    let old_b = SelectObject(mem_dc, GetStockObject(NULL_BRUSH as i32));
    Rectangle(mem_dc, 0, 0, width, height);
    SelectObject(mem_dc, old_b);
    SelectObject(mem_dc, old_p);
    DeleteObject(border_pen as *mut _);

    // Typography with authentic Riot Games "Beaufort for LOL" font
    let font_brand = create_riot_font(12, true);
    let font_header = create_riot_font(24, true);
    let font_sub = create_riot_sub_font(13, false);
    let font_name_large = create_riot_font(21, true);
    let font_tag = create_riot_sub_font(15, false);
    let font_server_badge = create_riot_font(22, true);
    let font_meta = create_riot_sub_font(12, true);

    // Anti-aliased Vector Icon Fonts
    let font_icon_play = create_vector_icon_font(14);
    let font_icon_trash = create_vector_icon_font(13);
    let font_icon_window = create_vector_icon_font(11);

    SetBkMode(mem_dc, TRANSPARENT as i32);

    let (active_acc, config, hov, feedback, is_err, scroll_offset, app_icon_small) = {
        if let Some(state_arc) = GUI_STATE.get() {
            if let Ok(state) = state_arc.lock() {
                let conf = state.storage.load_config();
                (
                    state.active_account.clone(),
                    conf,
                    state.hovered,
                    state.feedback_text.clone(),
                    state.feedback_is_error,
                    state.scroll_offset,
                    state.app_icon_small,
                )
            } else { return; }
        } else { return; }
    };

    // 3. Titlebar (Height: 44px)
    let titlebar_h = 44;
    let logo_x = 22;
    let logo_y = 14;
    if app_icon_small != 0 as HICON {
        DrawIconEx(
            mem_dc,
            logo_x,
            logo_y,
            app_icon_small,
            16,
            16,
            0,
            0 as HBRUSH,
            DI_NORMAL_FLAG,
        );
    } else {
        draw_round_rect(mem_dc, logo_x, logo_y, logo_x + 16, logo_y + 16, 3, 0x00101010, 0x003A3A3A, 1);
    }

    SelectObject(mem_dc, font_brand as *mut _);
    SetTextColor(mem_dc, COLOR_TEXT_WHITE);
    let brand_w = to_wstring("R I O T   F A S T   L O G I N");
    let mut brand_rect = RECT { left: logo_x + 24, top: 16, right: 340, bottom: 32 };
    DrawTextW(mem_dc, brand_w.as_ptr(), -1, &mut brand_rect, (DT_LEFT | DT_SINGLELINE) as u32);

    // Titlebar Window Controls
    let close_btn_w = 44;
    let close_btn_x = width - close_btn_w;
    let min_btn_x = close_btn_x - close_btn_w;

    let is_close_hover = hov == Some(HitArea::BtnClose);
    let is_min_hover = hov == Some(HitArea::BtnMinimize);

    if is_min_hover {
        let hov_brush = CreateSolidBrush(0x00242220);
        let min_rc = RECT { left: min_btn_x, top: 1, right: min_btn_x + close_btn_w, bottom: titlebar_h };
        FillRect(mem_dc, &min_rc, hov_brush as *mut _);
        DeleteObject(hov_brush as *mut _);
    }
    SelectObject(mem_dc, font_icon_window as *mut _);
    SetTextColor(mem_dc, if is_min_hover { COLOR_TEXT_WHITE } else { COLOR_TEXT_GRAY });
    let min_w = to_wstring(GLYPH_MINIMIZE);
    let mut min_rect = RECT { left: min_btn_x, top: 0, right: min_btn_x + close_btn_w, bottom: titlebar_h };
    DrawTextW(mem_dc, min_w.as_ptr(), -1, &mut min_rect, (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32);

    if is_close_hover {
        let red_brush = CreateSolidBrush(COLOR_CLOSE_HOVER);
        let close_rc = RECT { left: close_btn_x, top: 1, right: width - 1, bottom: titlebar_h };
        FillRect(mem_dc, &close_rc, red_brush as *mut _);
        DeleteObject(red_brush as *mut _);
    }
    SelectObject(mem_dc, font_icon_window as *mut _);
    SetTextColor(mem_dc, if is_close_hover { 0x00FFFFFF } else { COLOR_TEXT_GRAY });
    let close_w = to_wstring(GLYPH_CLOSE);
    let mut close_rect = RECT { left: close_btn_x, top: 0, right: width, bottom: titlebar_h };
    DrawTextW(mem_dc, close_w.as_ptr(), -1, &mut close_rect, (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32);

    // 4. Header: "Choose an account"
    let content_start_y = 65;
    SelectObject(mem_dc, font_header as *mut _);
    SetTextColor(mem_dc, COLOR_TEXT_WHITE);
    let title_w = to_wstring("Choose an account");
    let mut title_rect = RECT { left: 24, top: content_start_y, right: width - 24, bottom: content_start_y + 30 };
    DrawTextW(mem_dc, title_w.as_ptr(), -1, &mut title_rect, (DT_LEFT | DT_SINGLELINE) as u32);

    SelectObject(mem_dc, font_sub as *mut _);
    SetTextColor(mem_dc, COLOR_TEXT_GRAY);
    let sub_w = to_wstring("Click any account to switch and play instantly");
    let mut sub_rect = RECT { left: 24, top: content_start_y + 32, right: width - 24, bottom: content_start_y + 52 };
    DrawTextW(mem_dc, sub_w.as_ptr(), -1, &mut sub_rect, (DT_LEFT | DT_SINGLELINE) as u32);

    // 5. Account Cards
    let card_x = 24;
    let card_w = width - 48;
    let card_h = 76;
    let card_spacing = 10;
    let list_start_y = content_start_y + 64;

    let active_game_name = active_acc.as_ref().map(|a| a.game_name.to_lowercase()).unwrap_or_default();
    let active_tag_line = active_acc.as_ref().map(|a| a.tag_line.to_lowercase()).unwrap_or_default();

    if config.accounts.is_empty() {
        let box_y = list_start_y;
        let box_h = 175;
        draw_round_rect(mem_dc, card_x, box_y, card_x + card_w, box_y + box_h, 10, COLOR_CARD_BG, COLOR_CARD_BORDER, 1);

        SelectObject(mem_dc, font_icon_play as *mut _);
        SetTextColor(mem_dc, COLOR_RIOT_RED);
        let icon_w = to_wstring("\u{E77B}");
        let mut icon_rc = RECT { left: card_x, top: box_y + 22, right: card_x + card_w, bottom: box_y + 52 };
        DrawTextW(mem_dc, icon_w.as_ptr(), -1, &mut icon_rc, (DT_CENTER | DT_SINGLELINE) as u32);

        SelectObject(mem_dc, font_name_large as *mut _);
        SetTextColor(mem_dc, COLOR_TEXT_WHITE);
        let msg_title = to_wstring("No Accounts Linked Yet");
        let mut t_rc = RECT { left: card_x, top: box_y + 56, right: card_x + card_w, bottom: box_y + 82 };
        DrawTextW(mem_dc, msg_title.as_ptr(), -1, &mut t_rc, (DT_CENTER | DT_SINGLELINE) as u32);

        SelectObject(mem_dc, font_sub as *mut _);
        SetTextColor(mem_dc, COLOR_TEXT_GRAY);
        let msg_desc = to_wstring("1. Open Riot Client and sign into your account.\n2. Ensure 'Stay signed in' is checked.\n3. Your account will appear here automatically!");
        let mut d_rc = RECT { left: card_x + 16, top: box_y + 90, right: card_x + card_w - 16, bottom: box_y + 160 };
        DrawTextW(mem_dc, msg_desc.as_ptr(), -1, &mut d_rc, (DT_CENTER | DT_WORDBREAK) as u32);

        let add_btn_y = box_y + box_h + 12;
        let add_btn_h = 42;
        let is_add_hover = hov == Some(HitArea::AddAccount);
        let add_bg = if is_add_hover { COLOR_CARD_HOVER } else { 0x00141210 };
        let add_border = if is_add_hover { COLOR_RIOT_RED } else { 0x002E2822 };
        draw_round_rect(mem_dc, card_x, add_btn_y, card_x + card_w, add_btn_y + add_btn_h, 8, add_bg, add_border, if is_add_hover { 2 } else { 1 });
        SelectObject(mem_dc, font_sub as *mut _);
        SetTextColor(mem_dc, if is_add_hover { COLOR_TEXT_WHITE } else { COLOR_TEXT_GRAY });
        let add_text_w = to_wstring("+  Open Riot Client to Sign In");
        let mut add_rc = RECT { left: card_x, top: add_btn_y, right: card_x + card_w, bottom: add_btn_y + add_btn_h };
        DrawTextW(mem_dc, add_text_w.as_ptr(), -1, &mut add_rc, (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32);
    } else {
        let clip_rgn = CreateRectRgn(card_x - 4, list_start_y - 2, card_x + card_w + 4, height - 32);
        SelectClipRgn(mem_dc, clip_rgn);

        for i in 0..config.accounts.len() {
            let acc = &config.accounts[i];
            let item_y = list_start_y - scroll_offset + (i as i32 * (card_h + card_spacing));
            let card_cy = item_y + (card_h / 2);

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
                COLOR_RIOT_RED
            } else if is_current {
                0x004A3A2C
            } else {
                COLOR_CARD_BORDER
            };

            // Draw Card Frame
            draw_round_rect(
                mem_dc,
                card_x,
                item_y,
                card_x + card_w,
                item_y + card_h,
                8,
                bg_color,
                border_color,
                if any_hover { 2 } else { 1 },
            );

        // =========================================================================
        // ACCOUNT SPECIFIC OFFICIAL PROFILE ICON (SUMMONER ICON)
        // =========================================================================
        let av_size = 48;
        let av_x = card_x + 14;
        let av_y = card_cy - (av_size / 2);

        // Get or load this account's specific profile icon (.ico)
        let acc_icon: HICON = {
            if let Some(state_arc) = GUI_STATE.get() {
                if let Ok(mut state) = state_arc.lock() {
                    if let Some(&icon) = state.account_icons.get(&acc.id) {
                        icon
                    } else {
                        // Check if account-specific icon exists: %APPDATA%\RiotAccountSwitcher\icons\{acc.id}.ico
                        let ico_file = state.icons_dir.join(format!("{}.ico", acc.id));
                        let mut loaded_icon = 0 as HICON;
                        if ico_file.exists() {
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
                        loaded_icon
                    }
                } else { 0 as HICON }
            } else { 0 as HICON }
        };

        // Avatar Background Box with smooth rounded corners
        draw_round_rect(
            mem_dc,
            av_x,
            av_y,
            av_x + av_size,
            av_y + av_size,
            8,
            0x00141210,
            if is_current { COLOR_RIOT_RED } else { COLOR_CARD_BORDER },
            if is_current { 2 } else { 1 },
        );

        // Render account's specific profile icon!
        if acc_icon != 0 as HICON {
            DrawIconEx(
                mem_dc,
                av_x + 1,
                av_y + 1,
                acc_icon,
                46,
                46,
                0,
                0 as HBRUSH,
                DI_NORMAL_FLAG,
            );
        }

        // =========================================================================
        // BIGGER ACCOUNT NAME & SERVER CENTERED ON THE SAME LINE
        // =========================================================================
        let text_x = card_x + 76;

        // 1. Bigger Account Name (19px Bold)
        SelectObject(mem_dc, font_name_large as *mut _);
        SetTextColor(mem_dc, if any_hover { COLOR_TEXT_WHITE } else { 0x00F0F0F0 });
        let name_str = &acc.game_name;
        let name_w = to_wstring(name_str);
        let mut name_sz: SIZE = std::mem::zeroed();
        GetTextExtentPoint32W(mem_dc, name_w.as_ptr(), name_w.len() as i32 - 1, &mut name_sz);

        let line_y = card_cy - 13;
        let mut name_rect = RECT { left: text_x, top: line_y, right: text_x + name_sz.cx, bottom: line_y + 26 };
        DrawTextW(mem_dc, name_w.as_ptr(), -1, &mut name_rect, (DT_LEFT | DT_VCENTER | DT_SINGLELINE) as u32);

        // 2. TagLine (#000) on the same line
        SelectObject(mem_dc, font_tag as *mut _);
        SetTextColor(mem_dc, COLOR_TEXT_GRAY);
        let tag_x = text_x + name_sz.cx + 6;
        let tag_str = format!("#{}", acc.tag_line);
        let tag_w = to_wstring(&tag_str);
        let mut tag_sz: SIZE = std::mem::zeroed();
        GetTextExtentPoint32W(mem_dc, tag_w.as_ptr(), tag_w.len() as i32 - 1, &mut tag_sz);

        let mut tag_rect = RECT { left: tag_x, top: line_y + 1, right: tag_x + tag_sz.cx, bottom: line_y + 26 };
        DrawTextW(mem_dc, tag_w.as_ptr(), -1, &mut tag_rect, (DT_LEFT | DT_VCENTER | DT_SINGLELINE) as u32);

        // 3. Server / Region Badge on the SAME line!
        let badge_x = tag_x + tag_sz.cx + 10;
        let server_str = format!(" {} ", acc.region);
        let server_w = to_wstring(&server_str);
        let mut server_sz: SIZE = std::mem::zeroed();
        SelectObject(mem_dc, font_server_badge as *mut _);
        GetTextExtentPoint32W(mem_dc, server_w.as_ptr(), server_w.len() as i32 - 1, &mut server_sz);

        let s_badge_w = server_sz.cx + 12;
        let s_badge_h = 28;
        let s_badge_y = card_cy - (s_badge_h / 2);

        draw_round_rect(
            mem_dc,
            badge_x,
            s_badge_y,
            badge_x + s_badge_w,
            s_badge_y + s_badge_h,
            5,
            0x00221E14,
            0x003A3222,
            1,
        );

        SetTextColor(mem_dc, COLOR_RIOT_GOLD);
        let mut server_rect = RECT { left: badge_x, top: s_badge_y, right: badge_x + s_badge_w, bottom: s_badge_y + s_badge_h };
        DrawTextW(mem_dc, server_w.as_ptr(), -1, &mut server_rect, (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32);

        // =========================================================================
        // RIGHT SIDE: iOS VECTOR CONTROLS
        // =========================================================================
        // 1. iOS Play Button (34px diameter circular glassmorphism button)
        let play_cx = card_x + card_w - 30;
        draw_ios_play_button(mem_dc, play_cx, card_cy, 17, is_play_hover || is_card_hover, font_icon_play);

        // 2. iOS Trash Can Button (28px size)
        let trash_cx = play_cx - 40;
        draw_ios_trash_button(mem_dc, trash_cx, card_cy, 28, is_del_hover, font_icon_trash);

        // 3. Modern iOS Active Indicator (Apple Green Pulse Dot)
        if is_current {
            let active_dot_cx = trash_cx - 26;
            draw_ios_active_indicator(mem_dc, active_dot_cx, card_cy);
        }
    }

    // Draw "+ Add / Link Another Account" button
    let add_btn_y = list_start_y - scroll_offset + (config.accounts.len() as i32 * (card_h + card_spacing));
    let add_btn_h = 42;
    let is_add_hover = hov == Some(HitArea::AddAccount);

    let add_bg = if is_add_hover { COLOR_CARD_HOVER } else { 0x00141210 };
    let add_border = if is_add_hover { COLOR_RIOT_RED } else { 0x002E2822 };

    draw_round_rect(
        mem_dc,
        card_x,
        add_btn_y,
        card_x + card_w,
        add_btn_y + add_btn_h,
        8,
        add_bg,
        add_border,
        if is_add_hover { 2 } else { 1 },
    );

    SelectObject(mem_dc, font_sub as *mut _);
    SetTextColor(mem_dc, if is_add_hover { COLOR_TEXT_WHITE } else { COLOR_TEXT_GRAY });
    let add_text_w = to_wstring("+  Add / Link Another Account");
    let mut add_rc = RECT {
        left: card_x,
        top: add_btn_y,
        right: card_x + card_w,
        bottom: add_btn_y + add_btn_h,
    };
    DrawTextW(mem_dc, add_text_w.as_ptr(), -1, &mut add_rc, (DT_CENTER | DT_VCENTER | DT_SINGLELINE) as u32);

    SelectClipRgn(mem_dc, 0 as HRGN);
    DeleteObject(clip_rgn as *mut _);
}

    // 6. Minimalist Status Bar at bottom
    let status_y = height - 28;
    SelectObject(mem_dc, font_meta as *mut _);
    SetTextColor(mem_dc, if is_err { COLOR_RIOT_RED } else { COLOR_TEXT_MUTED });
    let fb_w = to_wstring(&feedback);
    let mut fb_rect = RECT { left: 24, top: status_y, right: width - 24, bottom: status_y + 18 };
    DrawTextW(mem_dc, fb_w.as_ptr(), -1, &mut fb_rect, (DT_CENTER | DT_SINGLELINE) as u32);

    // Blit to window
    BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

    // Clean up
    SelectObject(mem_dc, old_bmp);
    DeleteObject(mem_bmp as *mut _);
    DeleteDC(mem_dc);

    DeleteObject(font_brand as *mut _);
    DeleteObject(font_header as *mut _);
    DeleteObject(font_sub as *mut _);
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

    let close_btn_w = 44;
    let close_btn_x = width - close_btn_w;
    let min_btn_x = close_btn_x - close_btn_w;

    let mut new_hit: Option<HitArea> = None;

    if y >= 0 && y <= 44 {
        if x >= close_btn_x && x <= width {
            new_hit = Some(HitArea::BtnClose);
        } else if x >= min_btn_x && x < close_btn_x {
            new_hit = Some(HitArea::BtnMinimize);
        } else {
            new_hit = Some(HitArea::TitleBar);
        }
    } else {
        let card_x = 24;
        let card_w = width - 48;
        let card_h = 76;
        let card_spacing = 10;
        let list_start_y = 65 + 64;

        let (acc_count, scroll_offset) = {
            if let Some(state_arc) = GUI_STATE.get() {
                if let Ok(state) = state_arc.lock() {
                    (state.storage.load_config().accounts.len(), state.scroll_offset)
                } else { (0, 0) }
            } else { (0, 0) }
        };

        let bottom_limit = rect.bottom - rect.top - 32;
        if y >= list_start_y && y <= bottom_limit {
            for i in 0..acc_count {
                let item_y = list_start_y - scroll_offset + (i as i32 * (card_h + card_spacing));
                if x >= card_x && x <= card_x + card_w && y >= item_y && y <= item_y + card_h {
                    let card_cy = item_y + (card_h / 2);
                    let play_cx = card_x + card_w - 30;
                    let trash_cx = play_cx - 40;

                    if (x - play_cx).pow(2) + (y - card_cy).pow(2) <= 19 * 19 {
                        new_hit = Some(HitArea::PlayBtn(i));
                    } else if (x - trash_cx).abs() <= 15 && (y - card_cy).abs() <= 15 {
                        new_hit = Some(HitArea::Delete(i));
                    } else {
                        new_hit = Some(HitArea::Account(i));
                    }
                    break;
                }
            }

            if acc_count == 0 {
                let box_y = list_start_y;
                let box_h = 175;
                let add_btn_y = box_y + box_h + 12;
                let add_btn_h = 42;
                if x >= card_x && x <= card_x + card_w && y >= add_btn_y && y <= add_btn_y + add_btn_h {
                    new_hit = Some(HitArea::AddAccount);
                }
            } else if new_hit.is_none() {
                let add_btn_y = list_start_y - scroll_offset + (acc_count as i32 * (card_h + card_spacing));
                let add_btn_h = 42;
                if x >= card_x && x <= card_x + card_w && y >= add_btn_y && y <= add_btn_y + add_btn_h {
                    new_hit = Some(HitArea::AddAccount);
                }
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

    let hit = {
        if let Some(state_arc) = GUI_STATE.get() {
            if let Ok(state) = state_arc.lock() {
                state.hovered
            } else { None }
        } else { None }
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
            HitArea::AddAccount => {
                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        state.feedback_text = "Preparing Riot Client login...".to_string();
                        state.feedback_is_error = false;
                    }
                }
                InvalidateRect(hwnd, null_mut(), 0);
                UpdateWindow(hwnd);

                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(state) = state_arc.lock() {
                        state.riot.prepare_clean_login();
                        let _ = state.riot.launch("riot_client");
                    }
                }

                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        state.feedback_text = "Sign into your account with 'Stay signed in'!".to_string();
                        state.feedback_is_error = false;
                    }
                }
                InvalidateRect(hwnd, null_mut(), 0);
            }
            HitArea::Account(idx) | HitArea::PlayBtn(idx) => {
                let (target_id, target_name) = {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(state) = state_arc.lock() {
                            let conf = state.storage.load_config();
                            if idx < conf.accounts.len() {
                                (conf.accounts[idx].id.clone(), conf.accounts[idx].game_name.clone())
                            } else { return; }
                        } else { return; }
                    } else { return; }
                };

                if let Some(state_arc) = GUI_STATE.get() {
                    if let Ok(mut state) = state_arc.lock() {
                        state.feedback_text = format!("Launching {}...", target_name);
                        state.feedback_is_error = false;
                    }
                }
                InvalidateRect(hwnd, null_mut(), 0);
                UpdateWindow(hwnd);

                RiotManager::kill_riot_processes();

                let res = {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(state) = state_arc.lock() {
                            state.storage.apply_profile(&target_id, &state.riot.settings_yaml_path)
                        } else { Err("State lock error".to_string()) }
                    } else { Err("State not initialized".to_string()) }
                };

                match res {
                    Ok(_) => {
                        let _ = {
                            if let Some(state_arc) = GUI_STATE.get() {
                                if let Ok(state) = state_arc.lock() {
                                    state.riot.launch("riot_client")
                                } else { Ok(()) }
                            } else { Ok(()) }
                        };

                        if let Some(state_arc) = GUI_STATE.get() {
                            if let Ok(mut state) = state_arc.lock() {
                                state.active_account = state.riot.get_active_account();
                                state.feedback_text = format!("Launched: {}", target_name);
                                state.feedback_is_error = false;
                            }
                        }
                    }
                    Err(e) => {
                        if let Some(state_arc) = GUI_STATE.get() {
                            if let Ok(mut state) = state_arc.lock() {
                                state.feedback_text = format!("Error: {}", e);
                                state.feedback_is_error = true;
                            }
                        }
                    }
                }
                InvalidateRect(hwnd, null_mut(), 0);
            }
            HitArea::Delete(idx) => {
                let target_id = {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(state) = state_arc.lock() {
                            let conf = state.storage.load_config();
                            if idx < conf.accounts.len() {
                                conf.accounts[idx].id.clone()
                            } else { return; }
                        } else { return; }
                    } else { return; }
                };

                let confirm_txt = to_wstring("Remove this account from the switcher?");
                let confirm_title = to_wstring("Confirm Remove");
                let confirm = MessageBoxW(hwnd, confirm_txt.as_ptr(), confirm_title.as_ptr(), MB_YESNO | MB_ICONQUESTION);
                if confirm == IDYES {
                    if let Some(state_arc) = GUI_STATE.get() {
                        if let Ok(mut state) = state_arc.lock() {
                            let _ = state.storage.delete_profile(&target_id);
                            state.account_icons.remove(&target_id);
                            state.feedback_text = "Account removed.".to_string();
                            state.feedback_is_error = false;
                        }
                    }
                    InvalidateRect(hwnd, null_mut(), 0);
                }
            }
        }
    }
}
