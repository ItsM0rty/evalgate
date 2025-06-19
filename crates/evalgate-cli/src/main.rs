use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use evalgate_cli::{run_suite, RunOptions};
use evalgate_core::{compare_reports, report, RunReport};
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    name = "evalgate",
    about = "Run model evaluation suites and enforce score gates"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Run {
        suite: PathBuf,
        #[arg(long, default_value_t = 1)]
        repeat: usize,
        #[arg(long)]
        no_cache: bool,
        #[arg(long)]
        update_snapshots: bool,
        #[arg(long,value_enum,default_value_t=RunFormat::Table)]
        format: RunFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Compare {
        baseline: PathBuf,
        current: PathBuf,
        #[arg(long,value_enum,default_value_t=CompareFormat::Table)]
        format: CompareFormat,
        #[arg(long, default_value_t = 0.02)]
        tolerance: f64,
    },
}
#[derive(Clone, ValueEnum)]
enum RunFormat {
    Table,
    Json,
    Junit,
}
#[derive(Clone, ValueEnum)]
enum CompareFormat {
    Table,
    Json,
    Markdown,
}
#[tokio::main]
async fn main() -> Result<()> {
    let failed = match Cli::parse().command {
        Command::Run {
            suite,
            repeat,
            no_cache,
            update_snapshots,
            format,
            output,
        } => {
            let text = tokio::fs::read_to_string(&suite)
                .await
                .with_context(|| format!("read {}", suite.display()))?;
            let parsed = evalgate_core::parse_suite(&text).map_err(anyhow::Error::msg)?;
            let root = suite
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf();
            let reports = run_suite(
                &parsed,
                &RunOptions {
                    repeat,
                    no_cache,
                    update_snapshots,
                    root,
                },
            )
            .await?;
            let rendered = reports
                .iter()
                .map(|r| match format {
                    RunFormat::Table => Ok(report::terminal(r)),
                    RunFormat::Json => report::json(r),
                    RunFormat::Junit => Ok(report::junit(r)),
                })
                .collect::<Result<Vec<_>, String>>()
                .map_err(anyhow::Error::msg)?
                .join("\n");
            if let Some(path) = output {
                tokio::fs::write(path, rendered).await?
            } else {
                print!("{rendered}")
            }
            reports.iter().any(|r| !r.passed)
        }
        Command::Compare {
            baseline,
            current,
            format,
            tolerance,
        } => {
            let base: RunReport = serde_json::from_slice(&tokio::fs::read(baseline).await?)?;
            let current: RunReport = serde_json::from_slice(&tokio::fs::read(current).await?)?;
            let comparison = compare_reports(&base, &current, tolerance);
            match format {
                CompareFormat::Json => println!("{}", serde_json::to_string_pretty(&comparison)?),
                CompareFormat::Markdown => print!("{}", report::markdown_comparison(&comparison)),
                CompareFormat::Table => {
                    println!("Score delta: {:+.4}", comparison.score_delta);
                    for item in &comparison.regressions {
                        println!("REGRESSION  {:<24} {:+.4}", item.name, item.delta)
                    }
                    for item in &comparison.fixes {
                        println!("FIX         {:<24} {:+.4}", item.name, item.delta)
                    }
                }
            }
            comparison.regression
        }
    };
    if failed {
        std::process::exit(1)
    }
    Ok(())
}
