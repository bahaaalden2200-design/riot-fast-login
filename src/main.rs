#![windows_subsystem = "windows"]

mod gui;
mod riot;
mod storage;

use riot::RiotManager;
use storage::StorageManager;
use std::env;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(dwProcessId: u32) -> i32;
}

const ATTACH_PARENT_PROCESS: u32 = 0xFFFFFFFF;

/// Enforce Single-Instance behavior (like Riot Games Client)
/// If an instance is already running, restore its window, bring to foreground, and exit.
fn ensure_single_instance() -> bool {
    unsafe {
        let mutex_name = gui::to_wstring("Local\\RiotFastLogin_SingleInstance_Mutex");
        let mutex = CreateMutexW(null_mut(), 1, mutex_name.as_ptr());
        if mutex == 0 as HANDLE {
            return true;
        }

        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mutex);

            // An instance is already running! Restore from tray and bring to foreground
            let class_name = gui::to_wstring(gui::WINDOW_CLASS_NAME);
            let hwnd = FindWindowW(class_name.as_ptr(), null_mut());
            if hwnd != 0 as HWND {
                gui::restore_from_tray(hwnd);
            }
            return false;
        }

        // Keep mutex handle open for process lifetime without closing it
        let _ = mutex;
        true
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        // Attach to caller's console to print CLI output
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
        run_cli(&args);
    } else {
        // Enforce Single-Instance behavior: prevent duplicate windows
        if !ensure_single_instance() {
            return;
        }

        // Open Native Dark Mode GUI
        gui::run_gui();
    }
}

fn run_cli(args: &[String]) {
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let storage = StorageManager::new();
    let riot = RiotManager::new();

    match command {
        "--list" | "-l" => {
            let config = storage.load_config();
            println!("\n================ Saved Riot Accounts ================");
            if config.accounts.is_empty() {
                println!("No saved accounts found.");
            } else {
                for (i, acc) in config.accounts.iter().enumerate() {
                    println!(
                        "[{}] ID: {:<18} | Alias: {:<16} | User: {:<12} | Region: {}",
                        i + 1,
                        acc.id,
                        acc.alias,
                        acc.username,
                        acc.region
                    );
                }
            }
            println!("====================================================\n");
        }
        "--status" => {
            match riot.get_active_account() {
                Some(acc) => {
                    println!(
                        "🟢 Active Session: {} (Username: {}, Region: {})",
                        acc.display_name(),
                        acc.username,
                        acc.region
                    );
                }
                None => {
                    println!("⚪ No active logged-in Riot session found.");
                }
            }
        }
        "--save" => {
            let custom_alias = args.get(2).map(|s| s.as_str());
            match riot.get_active_account() {
                Some(acc) => {
                    match storage.save_active_account(&acc, custom_alias, &riot.settings_yaml_path) {
                        Ok(p) => println!("✅ Successfully saved profile: {} [{}]", p.alias, p.id),
                        Err(e) => eprintln!("❌ Error saving account: {}", e),
                    }
                }
                None => eprintln!("❌ No active session found in Riot Client to save!"),
            }
        }
        "--switch" => {
            let target = match args.get(2) {
                Some(t) => t.as_str(),
                None => {
                    eprintln!("Usage: riot-account-switcher --switch <ID or Index>");
                    return;
                }
            };

            let config = storage.load_config();
            let profile_id = if let Ok(idx) = target.parse::<usize>() {
                if idx > 0 && idx <= config.accounts.len() {
                    config.accounts[idx - 1].id.clone()
                } else {
                    eprintln!("Invalid index. Run --list to see available accounts.");
                    return;
                }
            } else {
                target.to_string()
            };

            println!("Closing Riot processes...");
            RiotManager::kill_riot_processes();

            match storage.apply_profile(&profile_id, &riot.settings_yaml_path) {
                Ok(p) => {
                    println!("✅ Session swapped to: {} [{}]", p.alias, p.id);
                    println!("Launching Riot Client...");
                    if let Err(e) = riot.launch("riot_client") {
                        eprintln!("Error launching: {}", e);
                    }
                }
                Err(e) => eprintln!("❌ Switch error: {}", e),
            }
        }
        "--kill" => {
            println!("Closing all Riot Games processes...");
            RiotManager::kill_riot_processes();
            println!("✅ Done.");
        }
        "--help" | "-h" => {
            println!("Riot Fast Login (Rust) - CLI options:");
            println!("  --list, -l         List all saved accounts");
            println!("  --status           Show currently active logged-in account");
            println!("  --save [alias]     Save current logged-in session");
            println!("  --switch <id/idx>  Switch to an account by ID or index and launch");
            println!("  --kill             Terminate all running Riot processes");
            println!("  --help, -h         Show this help message");
            println!("  (no arguments)     Launch the graphical Dark Mode window");
        }
        _ => {
            eprintln!("Unknown command: '{}'. Use --help for usage instructions.", command);
        }
    }
}
