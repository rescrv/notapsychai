use std::process;

use labradormail::run_from_server;

fn main() {
    if let Err(e) = run_from_server("http://localhost:3000/inbox") {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
