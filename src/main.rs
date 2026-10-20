use std::{error::Error, path::Path, process::ExitCode};

use mini_router::loader::load_connectors;

fn main() -> ExitCode {
    match load_connectors(Path::new("connectors")) {
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
    }
}
