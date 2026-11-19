#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--mock-admin") {
        // GUI-subsystem exe: print into the console it was started from.
        unsafe { windows_sys::Win32::System::Console::AttachConsole(u32::MAX) };
        std::process::exit(dbdev::mock_main(&args));
    }
    dbdev::run();
}
