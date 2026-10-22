//! Replay typed observations without loading native adapters or sending any input.
//! cargo run -p macro-core --example replay -- [profile.json] < events.jsonl
use macro_core::{Engine, Event, Profile};
use serde::Deserialize;
use serde_json::json;
use std::{error::Error, io::{self, BufRead, Write}};

#[derive(Deserialize)]
struct Input {
    event: Event,
    now_ms: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() > 1 {
        return Err("Usage: replay [profile.json] < events.jsonl".into());
    }
    let profile = if let Some(path) = args.first() {
        Profile::from_json(&std::fs::read_to_string(path)?)?
    } else {
        Profile::default()
    };
    let mut engine = Engine::new(profile)?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for (index, line) in stdin.lock().lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        let input: Input = serde_json::from_str(&line)
            .map_err(|error| format!("Invalid replay input at line {}: {error}", index + 1))?;
        let actions = engine.step(input.event, input.now_ms);
        serde_json::to_writer(&mut output, &json!({
            "actions": actions,
            "snapshot": engine.snapshot(),
        }))?;
        writeln!(&mut output)?;
        output.flush()?;
    }
    Ok(())
}
