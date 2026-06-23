#![forbid(unsafe_code)]

fn main() {
    let output = livespec_console_beads_fabro::run(std::env::args());
    println!("{}", output.message());
    std::process::exit(output.code());
}
