use anyhow::Result;
use run::app;
use run::cli;

fn main() -> Result<()> {
    let command = cli::parse()?;
    let exit_code = app::run(command)?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
