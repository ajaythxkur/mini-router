use std::{error::Error, path::Path, process::ExitCode};

use mini_router::loader::load_connectors;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    match args.as_slice() {
        [_, cmd] if cmd == "list" => match load_connectors(Path::new("connectors")) {
            Ok(connectors) => {
                for c in &connectors {
                    println!("{:<25} {:?}", c.id, c.pricing);
                }

                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e}");
                let mut cause = e.source();
                while let Some(c) = cause {
                    eprintln!("  caused by: {c}");
                    cause = c.source();
                }
                ExitCode::FAILURE
            }
        },
        [_, cmd, id, input] if cmd == "run" => ExitCode::SUCCESS,
        _ => {
            eprintln!("usage: mini-router list | run <connector-id> '<json-input>'");
            ExitCode::FAILURE
        }
    }
}
