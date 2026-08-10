//! 跨平台「弹窗提醒」模块
//!
//! 用途：token 获取失败退出前，用系统原生对话框提醒用户，避免
//! Windows 双击运行时控制台一闪而过、用户看不到提示的问题。
//!
//! 各平台策略（任何平台失败都静默回退，不影响主流程）：
//! - Windows: 原生 Win32 MessageBoxW（user32.dll，零依赖）
//! - macOS:   osascript 系统对话框（失败回退终端输出）
//! - Linux:   zenity / kdialog（常见桌面环境自带）

#[cfg(not(target_os = "windows"))]
use std::process::Command;

/// 在退出前弹出提醒对话框（阻塞，直到用户点击确定）
pub fn show_alert(title: &str, message: &str) {
    #[cfg(target_os = "windows")]
    {
        if show_windows_message_box(title, message) {
            return;
        }
        // 回退：原生 API 失败时（极少见），把信息打回终端
        eprintln!("\n⚠️  {}\n{}", title, message);
    }

    #[cfg(target_os = "macos")]
    {
        if show_macos_dialog(title, message) {
            return;
        }
        // 回退：osascript 不可用时（如非 GUI 会话），终端输出同样清晰
        eprintln!("\n⚠️  {}\n{}", title, message);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if show_linux_dialog(title, message) {
            return;
        }
        eprintln!("\n⚠️  {}\n{}", title, message);
    }
}

/// Windows: 调用原生 user32!MessageBoxW（MB_ICONWARNING | MB_OK | MB_TOPMOST）
#[cfg(target_os = "windows")]
fn show_windows_message_box(title: &str, message: &str) -> bool {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    // 编译期静态链接 user32.dll（Windows 系统必备库，无额外依赖）
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(
            hwnd: *const c_void,
            lp_text: *const u16,
            lp_caption: *const u16,
            u_type: u32,
        ) -> i32;
    }

    let wide_text: Vec<u16> = std::ffi::OsStr::new(message)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let wide_title: Vec<u16> = std::ffi::OsStr::new(title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    const MB_OK: u32 = 0x0000;
    const MB_ICONWARNING: u32 = 0x0030;
    const MB_TOPMOST: u32 = 0x40000;

    // 每次调用前先强制刷新控制台缓冲，确保文本日志先于弹窗可见
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();

    // 失败时返回 0（MessageBoxW 失败的唯一返回值）
    unsafe {
        MessageBoxW(
            ptr::null(),
            wide_text.as_ptr(),
            wide_title.as_ptr(),
            MB_OK | MB_ICONWARNING | MB_TOPMOST,
        ) != 0
    }
}

/// macOS: 用 osascript 弹系统对话框（阻塞直到用户点击）
#[cfg(target_os = "macos")]
fn show_macos_dialog(title: &str, message: &str) -> bool {
    // 转义脚本字符串：反斜杠、引号、换行
    let esc = |s: &str| {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    };
    let script = format!(
        "display dialog \"{}\" with title \"{}\" with icon caution buttons {{\"确定\"}} default button \"确定\"",
        esc(message),
        esc(title)
    );
    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Linux: 优先 zenity，回退 kdialog
#[cfg(all(unix, not(target_os = "macos")))]
fn show_linux_dialog(title: &str, message: &str) -> bool {
    let zenity = Command::new("zenity")
        .args(["--warning", "--title", title, "--text", message])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if zenity {
        return true;
    }
    Command::new("kdialog")
        .args(["--warningyesno", message, "--title", title])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
