use clap::{Args, Parser, Subcommand};
use miette::Report;
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use txtql::{Options, Query};

/// Extract structured JSON from free text with readable grammar rules.
#[derive(Parser)]
#[command(version, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    run: RunArgs,
}

#[derive(Subcommand)]
enum Command {
    /// Run a query on some text and print the JSON result (the default).
    Run(RunArgs),
    /// Check a query for errors and lints; with input, also list output-changing ambiguities (up to 20).
    Check(CheckArgs),
    /// Run the language server for editors, over standard input and output.
    Lsp,
}

/// `QUERY_FILE [INPUT]`, or `-e QUERY [INPUT]`.
#[derive(Args)]
struct Files {
    /// The query file followed by the input file (standard input when omitted or `-`).
    /// With `-e`, only the input file.
    #[arg(value_name = "FILES", num_args = 0..=2)]
    files: Vec<PathBuf>,
    /// Query text given inline instead of a query file.
    #[arg(short = 'e', long = "expr", value_name = "QUERY")]
    expr: Option<String>,
}

enum QuerySource {
    Inline(String),
    File(PathBuf),
}

impl Files {
    fn split(self) -> Result<(QuerySource, Option<PathBuf>), Report> {
        let mut files = self.files.into_iter();
        let query = match self.expr {
            Some(e) => QuerySource::Inline(e),
            None => QuerySource::File(files.next().ok_or_else(|| {
                miette::miette!(help = "usage: txtql QUERY_FILE [INPUT] or txtql -e 'QUERY' [INPUT]", "no query given")
            })?),
        };
        let input = files.next();
        if let Some(extra) = files.next() {
            return Err(miette::miette!("unexpected extra argument {}", extra.display()));
        }
        Ok((query, input))
    }
}

#[derive(Args)]
struct RunArgs {
    #[command(flatten)]
    files: Files,
    /// Treat output-changing ambiguity and repeated keys as errors.
    #[arg(long)]
    strict: bool,
    /// Give up after this many parser steps.
    #[arg(long, default_value_t = Options::default().max_steps)]
    max_steps: u64,
    /// Maximum nesting depth of rule matches.
    #[arg(long, default_value_t = Options::default().max_depth)]
    max_depth: usize,
    /// Print compact JSON instead of pretty-printed JSON.
    #[arg(long)]
    compact: bool,
    /// Skip the ambiguity analysis.
    #[arg(long)]
    no_ambiguity_check: bool,
}

#[derive(Args)]
struct CheckArgs {
    #[command(flatten)]
    files: Files,
}

/// Stack size for the worker thread: deep recursive matches need room.
const STACK_SIZE: usize = 1 << 30;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let worker = std::thread::Builder::new().stack_size(STACK_SIZE).spawn(move || real_main(cli));
    match worker.map(|w| w.join()) {
        Ok(Ok(code)) => code,
        Ok(Err(_)) => ExitCode::from(101),
        Err(e) => {
            eprintln!("error: could not start worker thread: {e}");
            ExitCode::FAILURE
        }
    }
}

fn eprint_reports(reports: &[Report]) {
    for r in reports {
        eprintln!("{r:?}");
    }
}

fn read_query(src: &QuerySource) -> Result<(String, String), Report> {
    match src {
        QuerySource::Inline(e) => Ok(("<query>".into(), e.clone())),
        QuerySource::File(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| miette::miette!(code = "txtql::io", "cannot read query file {}: {e}", path.display()))?;
            Ok((path.display().to_string(), text))
        }
    }
}

fn read_input(path: &Option<PathBuf>) -> Result<(String, String), Report> {
    let (name, bytes) = match path {
        Some(p) if p.as_os_str() != "-" => {
            let bytes = std::fs::read(p)
                .map_err(|e| miette::miette!(code = "txtql::io", "cannot read input file {}: {e}", p.display()))?;
            (p.display().to_string(), bytes)
        }
        _ => {
            if std::io::stdin().is_terminal() {
                eprintln!("(reading input from the terminal; press Ctrl-D when done)");
            }
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|e| miette::miette!(code = "txtql::io", "cannot read standard input: {e}"))?;
            ("<stdin>".to_string(), bytes)
        }
    };
    match String::from_utf8(bytes) {
        Ok(s) => Ok((name, s)),
        Err(e) => {
            let at = e.utf8_error().valid_up_to();
            Err(miette::miette!(
                code = "txtql::io::utf8",
                help = "txtql reads UTF-8 text; convert the file first, e.g. with `iconv -t UTF-8`",
                "{name} is not valid UTF-8 (invalid byte at offset {at})"
            ))
        }
    }
}

fn compile(qname: &str, qtext: &str) -> Option<Query> {
    match Query::compile(qtext) {
        Ok(q) => {
            eprint_reports(&txtql::warning_reports(&q.warnings, qname, qtext));
            Some(q)
        }
        Err(e) => {
            eprint_reports(&e.reports(qname, qtext));
            None
        }
    }
}

fn real_main(cli: Cli) -> ExitCode {
    let result = match cli.command {
        Some(Command::Run(args)) => run(args),
        Some(Command::Check(args)) => check(args),
        Some(Command::Lsp) => txtql::lsp::run_stdio()
            .map(|()| ExitCode::SUCCESS)
            .map_err(|e| miette::miette!(code = "txtql::lsp", "language server failed: {e}")),
        None => run(cli.run),
    };
    match result {
        Ok(code) => code,
        Err(report) => {
            eprintln!("{report:?}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: RunArgs) -> Result<ExitCode, Report> {
    let (source, input_path) = args.files.split()?;
    let (qname, qtext) = read_query(&source)?;
    let Some(query) = compile(&qname, &qtext) else { return Ok(ExitCode::FAILURE) };
    let (iname, input) = read_input(&input_path)?;
    let opts = Options {
        max_steps: args.max_steps,
        max_depth: args.max_depth,
        strict: args.strict,
        check_ambiguity: !args.no_ambiguity_check,
        ..Options::default()
    };
    match query.run(&input, &opts) {
        Ok(out) => {
            eprint_reports(&txtql::ambiguity_reports(&out.ambiguities, &iname, &input));
            eprint_reports(&txtql::repeated_key_reports(&out.repeated_keys, &qname, &qtext, &iname, &input));
            if opts.check_ambiguity && !out.ambiguity_check_complete {
                eprintln!("note: the ambiguity check stopped early on this input; some ambiguities may be unreported");
            }
            let json =
                if args.compact { serde_json::to_string(&out.value) } else { serde_json::to_string_pretty(&out.value) };
            txtql::drop_deep(out.value);
            // A closed pipe (`txtql … | head`) is not an error worth a panic.
            let mut stdout = std::io::stdout().lock();
            match writeln!(stdout, "{}", json.expect("JSON values always serialize")).and_then(|_| stdout.flush()) {
                Ok(()) => Ok(ExitCode::SUCCESS),
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(ExitCode::SUCCESS),
                Err(e) => Err(miette::miette!(code = "txtql::io", "cannot write output: {e}")),
            }
        }
        Err(e) => {
            eprint_reports(&e.reports(&qname, &qtext, &iname, &input));
            Ok(ExitCode::FAILURE)
        }
    }
}

fn check(args: CheckArgs) -> Result<ExitCode, Report> {
    let (source, input_path) = args.files.split()?;
    let (qname, qtext) = read_query(&source)?;
    let Some(query) = compile(&qname, &qtext) else { return Ok(ExitCode::FAILURE) };
    let Some(path) = &input_path else {
        eprintln!("query OK ({} rules)", query.ast.rules.len());
        return Ok(ExitCode::SUCCESS);
    };
    let (iname, input) = read_input(&Some(path.clone()))?;
    let opts = Options { ambiguity_steps: u64::MAX / 2, ..Options::default() };
    match query.run(&input, &opts) {
        Ok(out) => {
            eprint_reports(&txtql::ambiguity_reports(&out.ambiguities, &iname, &input));
            eprint_reports(&txtql::repeated_key_reports(&out.repeated_keys, &qname, &qtext, &iname, &input));
            let (a, k) = (out.ambiguities.len(), out.repeated_keys.len());
            if a == 0 && k == 0 {
                eprintln!("query OK; no output-changing ambiguity on this input");
                return Ok(ExitCode::SUCCESS);
            }
            let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
            let mut found = Vec::new();
            if a > 0 {
                found.push(plural(a, "output-changing ambiguity", "output-changing ambiguities"));
            }
            if k > 0 {
                found.push(plural(k, "repeated key", "repeated keys"));
            }
            eprintln!("{} found", found.join(" and "));
            Ok(ExitCode::from(2))
        }
        Err(e) => {
            eprint_reports(&e.reports(&qname, &qtext, &iname, &input));
            Ok(ExitCode::FAILURE)
        }
    }
}
