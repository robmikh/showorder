use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(short = 'n', long = "max-count", default_value_t = 5)]
    pub max_count: usize,
    #[clap(short, long)]
    pub track_number: Option<u64>,
    #[clap(short = 'm', long = "max")]
    pub max_distance: Option<usize>,
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    ListTracks {
        mkv_path: String,
    },
    List {
        file_type: FileType,
        input_path: String,
    },
    Dump {
        dump_type: DumpType,
        mkv_path: String,
        output_path: String,
    },
    Match {
        mkv_path: String,
        reference_path: String,
    },
}

#[derive(Debug)]
pub enum DumpType {
    Png,
    Bgra8,
    Block,
}

pub struct DumpTypeParseError(pub String);
impl Display for DumpTypeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown dump type \"{}\".", self.0)
    }
}
impl Debug for DumpTypeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{{}}}", self.0)
    }
}
impl std::error::Error for DumpTypeParseError {}

impl FromStr for DumpType {
    type Err = DumpTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "png" => Ok(DumpType::Png),
            "bgra8" => Ok(DumpType::Bgra8),
            "block" => Ok(DumpType::Block),
            _ => Err(DumpTypeParseError(s.to_string())),
        }
    }
}

#[derive(Debug)]
pub enum FileType {
    Mkv,
    Srt,
}

pub struct FileTypeParseError(pub String);
impl Display for FileTypeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown file type \"{}\".", self.0)
    }
}
impl Debug for FileTypeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{{}}}", self.0)
    }
}
impl std::error::Error for FileTypeParseError {}

impl FromStr for FileType {
    type Err = FileTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mkv" => Ok(FileType::Mkv),
            "srt" => Ok(FileType::Srt),
            _ => Err(FileTypeParseError(s.to_string())),
        }
    }
}
