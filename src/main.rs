use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

mod codegen;
mod imports;
mod parser;
mod source;

const EXIT_SUCCESS: u8 = 0;
const EXIT_ERROR: u8 = 1;
const EXIT_USAGE: u8 = 2;

const HELP: &str = "bam - generate C objects from .bam files

Usage:
  bam <COMMAND>

Commands:
  generate <input.bam>  Generate a C header/source pair
  help                  Show this help

Options:
  -h, --help            Show this help
  -V, --version         Show version information
";

const GENERATE_HELP: &str = "Generate a C header/source pair from a .bam file.

Usage:
  bam generate <input.bam>

Options:
  -h, --help            Show this help
";

fn main() -> ExitCode {
    ExitCode::from(run(env::args_os().skip(1).collect()))
}

fn run(args: Vec<OsString>) -> u8 {
    let Some(command) = args.first() else {
        print!("{HELP}");
        return EXIT_SUCCESS;
    };

    if is_flag(command, "-h", "--help") || command == "help" {
        print!("{HELP}");
        return EXIT_SUCCESS;
    }
    if is_flag(command, "-V", "--version") {
        println!("bam {}", env!("CARGO_PKG_VERSION"));
        return EXIT_SUCCESS;
    }

    match command.to_str() {
        Some("generate") => run_generate(&args[1..]),
        Some(other) => {
            eprintln!("bam: unknown command '{other}'\n\n{HELP}");
            EXIT_USAGE
        }
        None => {
            eprintln!("bam: command name is not valid Unicode\n\n{HELP}");
            EXIT_USAGE
        }
    }
}

fn run_generate(args: &[OsString]) -> u8 {
    if args.len() == 1 && is_flag(&args[0], "-h", "--help") {
        print!("{GENERATE_HELP}");
        return EXIT_SUCCESS;
    }

    let path_arg = match args {
        [path] => path,
        [separator, path] if separator == "--" => path,
        [] => {
            eprintln!("bam generate: missing input path\n\n{GENERATE_HELP}");
            return EXIT_USAGE;
        }
        _ => {
            eprintln!("bam generate: expected exactly one input path\n\n{GENERATE_HELP}");
            return EXIT_USAGE;
        }
    };

    let input_path = PathBuf::from(path_arg);
    if input_path
        .extension()
        .is_none_or(|extension| extension != "bam")
    {
        eprintln!("bam generate: input path must have a .bam extension");
        return EXIT_USAGE;
    }

    let project = match imports::load_project(&input_path) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("bam generate: {error}");
            return EXIT_ERROR;
        }
    };

    let object = match parser::parse_root(&project) {
        Ok(object) => object,
        Err(error) => {
            eprintln!("bam generate: {error}");
            return EXIT_ERROR;
        }
    };

    let generated = match codegen::generate(&project, &object) {
        Ok(generated) => generated,
        Err(error) => {
            eprintln!("bam generate: {error}");
            return EXIT_ERROR;
        }
    };
    if let Err(error) = codegen::write(&generated) {
        eprintln!("bam generate: {error}");
        return EXIT_ERROR;
    }

    println!(
        "Generated '{}' and '{}'",
        generated.header_path.display(),
        generated.source_path.display()
    );
    EXIT_SUCCESS
}

fn is_flag(value: &OsString, short: &str, long: &str) -> bool {
    value == short || value == long
}
