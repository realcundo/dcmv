use clap::Parser;
use std::path::PathBuf;

/// A terminal-based DICOM image viewer
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// DICOM file path(s) to display
    #[arg(value_name = "FILE")]
    pub files: Vec<PathBuf>,

    /// Output width in terminal columns
    #[arg(short = 'W', long)]
    pub width: Option<u32>,

    /// Output height in terminal rows
    #[arg(short = 'H', long)]
    pub height: Option<u32>,

    /// Show DICOM metadata
    #[arg(short, long)]
    pub verbose: bool,
}
