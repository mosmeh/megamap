mod printer;

use anyhow::Result;
use printer::PrinterBuilder;
use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(
    author = env!("CARGO_PKG_AUTHORS"),
    setting(clap::AppSettings::ColoredHelp),
    setting(clap::AppSettings::DeriveDisplayOrder),
)]
struct Opt {
    /// File(s) to highlight
    ///
    /// Use a "-" or no argument for standard input.
    file: Vec<PathBuf>,

    /// Explicitly set the language for syntax highlighting
    ///
    /// Languages can be specified as a name (e.g. rust) or an extension (e.g. rs).
    #[structopt(short, long)]
    language: Option<String>,

    /// Maximum number of columns
    #[structopt(short, long)]
    columns: Option<usize>,

    /// Tab width
    ///
    /// Specify 0 to pass tabs through.
    #[structopt(short, long)]
    tabs: Option<usize>,
}

fn main() -> Result<()> {
    let opt = Opt::from_args();

    let mut builder = PrinterBuilder::new();
    builder.true_color(true_color_is_enabled());
    if let Some(lang) = opt.language {
        builder.language(&lang);
    }
    if let Some(columns) = opt.columns {
        builder.columns(columns);
    }
    if let Some(tabs) = opt.tabs {
        builder.tabs(tabs);
    }

    let printer = builder.build();
    let mut stdout = io::stdout();

    if opt.file.is_empty() || (opt.file.len() == 1 && opt.file[0] == PathBuf::from("-")) {
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        printer.print_from_reader(&mut stdout, &mut stdin)?;
    } else {
        for file in opt.file {
            printer.print_file(&mut stdout, file)?;
        }
    }

    Ok(())
}

fn true_color_is_enabled() -> bool {
    std::env::var("COLORTERM")
        .map(|colorterm| match &colorterm[..] {
            "truecolor" | "24bit" => true,
            _ => false,
        })
        .unwrap_or(false)
}
