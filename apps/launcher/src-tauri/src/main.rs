#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Elevated helper mode: the launcher restarts itself with UAC for exactly these two commands.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 2 && args[0] == dbl::hosts::HELPER_FLAG {
        std::process::exit(dbl::hosts::helper_main(&args[1]));
    }
    if !dbl::winutil::single_instance(args.iter().any(|a| a == dbl::WAIT_INSTANCE_FLAG)) {
        return; // already running: its window was brought to the front
    }
    dbl::run();
}
