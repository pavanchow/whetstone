use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use whetstone::ir::parse;
use whetstone::passes::optimize;

#[derive(Parser)]
#[command(name = "whetstone", about = "A readable optimizing pass pipeline over a small IR")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Optimize an IR program and print the result.
    Opt {
        /// Path to a Whetstone IR text file.
        file: PathBuf,
        /// Print the before and after of every pass, not just the final result.
        #[arg(long)]
        show_passes: bool,
    },
}

/// A typed error covering everything that can go wrong before we
/// have a program to optimize. Nothing in this binary panics on bad
/// input, an unreadable file or a malformed IR line becomes one of
/// these and is reported on stderr.
#[derive(Debug)]
enum WhetstoneError {
    Io { path: PathBuf, source: std::io::Error },
    Parse(whetstone::ir::ParseError),
}

impl fmt::Display for WhetstoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WhetstoneError::Io { path, source } => {
                write!(f, "could not read {}: {source}", path.display())
            }
            WhetstoneError::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

impl std::error::Error for WhetstoneError {}

fn run(cli: Cli) -> Result<(), WhetstoneError> {
    match cli.command {
        Command::Opt { file, show_passes } => {
            let source = fs::read_to_string(&file).map_err(|source| WhetstoneError::Io {
                path: file.clone(),
                source,
            })?;
            let prog = parse(&source).map_err(WhetstoneError::Parse)?;
            let result = optimize(&prog);

            if show_passes {
                println!("=== original ===");
                println!("{}", result.original);
                println!();
                for report in &result.reports {
                    if report.before == report.after {
                        continue;
                    }
                    println!("=== {} (round {}) ===", report.name, report.iteration);
                    println!("--- before ---");
                    println!("{}", report.before);
                    println!("--- after ---");
                    println!("{}", report.after);
                    for note in &report.notes {
                        println!("  {note}");
                    }
                    println!();
                }
                println!(
                    "=== fixed point reached in {} round{} ===",
                    result.iterations,
                    if result.iterations == 1 { "" } else { "s" }
                );
                println!();
            }

            println!("{}", result.final_program);

            if !show_passes {
                let notes = result.all_notes();
                if !notes.is_empty() {
                    eprintln!();
                    eprintln!("changes:");
                    for note in notes {
                        eprintln!("  {note}");
                    }
                }
            }

            Ok(())
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
