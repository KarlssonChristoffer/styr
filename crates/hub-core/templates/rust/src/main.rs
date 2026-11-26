use clap::Parser;

/// {{name}}
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Who to greet.
    #[arg(short, long, default_value = "world")]
    name: String,
}

/// The one thing this starter does; replace it with the real work.
fn greeting(name: &str) -> String {
    format!("Hello, {name}!")
}

fn main() {
    let args = Args::parse();
    println!("{}", greeting(&args.name));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets_by_name() {
        assert_eq!(greeting("Ada"), "Hello, Ada!");
    }
}
