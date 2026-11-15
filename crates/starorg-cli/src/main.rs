fn main() -> std::process::ExitCode {
    // `starorg-codex status | head -1`: a closed stdout is not an error. println!
    // panics on EPIPE; exit quietly instead of printing a panic message.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if is_broken_pipe(info.payload()) {
            std::process::exit(0);
        }
        default_hook(info);
    }));
    starorg_cli::run(std::env::args_os())
}

fn is_broken_pipe(payload: &(dyn std::any::Any + Send)) -> bool {
    let msg = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    msg.starts_with("failed printing to stdout") && msg.contains("Broken pipe")
}
