mod args;
mod commands;
mod input;
mod output;

fn main() {
    match args::Args::parse().and_then(commands::run) {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("JSON value serializes")
        ),
        Err(error) => {
            eprintln!("{}", error.message);
            println!(
                "{}",
                serde_json::to_string_pretty(&error.body).expect("JSON value serializes")
            );
            std::process::exit(error.exit_code);
        }
    }
}
