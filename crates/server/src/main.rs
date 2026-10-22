use serde_json::Value;
use std::{error::Error, path::Path, process::ExitCode};

use mini_router_core::engine::{http_client, run};
use mini_router_core::loader::load_connectors;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    match cli(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            print_error(e.as_ref());
            ExitCode::FAILURE
        }
    }
}

async fn cli(args: &[String]) -> Result<(), Box<dyn Error>> {
    match args {
        [_, cmd] if cmd == "list" => {
            for c in load_connectors(Path::new("connectors"))? {
                println!("{:<25} {:?}", c.id, c.pricing);
            }
        }
        [_, cmd, id, input] if cmd == "run" => {
            let connectors = load_connectors(Path::new("connectors"))?;
            let Some(connector) = connectors.iter().find(|c| c.id == *id) else {
                return Err(format!("unknown connector: {id}").into());
            };
            let input = serde_json::from_str::<Value>(input)?;
            let value = run(&http_client()?, connector, &input).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        [_, cmd] if cmd == "serve" => mini_router::server::server("0.0.0.0:8080").await?,
        _ => {
            return Err(
                "usage: mini-router list | run <connector-id> '<json-input>' | serve".into(),
            );
        }
    }
    Ok(())
}

fn print_error(e: &dyn Error) {
    eprintln!("error: {e}");
    let mut cause = e.source();
    while let Some(c) = cause {
        eprintln!("  caused by: {c}");
        cause = c.source();
    }
}
