#![forbid(unsafe_code)]

#[cfg(all(not(test), not(coverage)))]
use std::io::IsTerminal;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    #[cfg(all(not(test), not(coverage)))]
    {
        if should_run_interactive_tui(&args) && std::io::stdout().is_terminal() {
            match console_tui::run_interactive_tui(
                &livespec_console_beads_fabro::demo_events(),
                "operator",
            ) {
                Ok(_effects) => {
                    std::process::exit(0);
                }
                Err(error) => {
                    eprintln!("tui error: {error}");
                    std::process::exit(1);
                }
            }
        }
    }
    let output = livespec_console_beads_fabro::run(args);
    println!("{}", output.message());
    std::process::exit(output.code());
}

#[cfg(all(not(test), not(coverage)))]
fn should_run_interactive_tui(args: &[String]) -> bool {
    let command = args.get(1).map(String::as_str);
    let mode = args.get(2).map(String::as_str);
    command == Some("tui") && mode != Some("--preview")
}
