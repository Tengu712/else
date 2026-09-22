use std::{env, fs, process};

mod parser;

fn main() {
    let args = env::args().skip(1).collect::<Vec<String>>();

    if args.is_empty() {
        // TODO: repl
        unimplemented!();
    }

    let content = match fs::read_to_string(&args[1]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };
    if let Err(e) = run(&content) {
        eprintln!("{e}");
        process::exit(1);
    }
}

fn run(code: &str) -> Result<(), String> {
    let _ = parser::parse(code)?;
    // TODO:
    Ok(())
}
