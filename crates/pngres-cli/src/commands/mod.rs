use std::process::ExitCode;

use clap::Subcommand;

mod capacity;
mod decode;
mod encode;
mod inspect;
mod remote;
#[cfg(feature = "serve")]
mod serve;

pub use capacity::CapacityArgs;
pub use decode::DecodeArgs;
pub use encode::EncodeArgs;
pub use inspect::InspectArgs;
pub use remote::RemoteArgs;
#[cfg(feature = "serve")]
pub use serve::ServeArgs;

#[derive(Subcommand)]
pub enum Command {
    /// Encode a file into a herd of monkes.
    Encode(EncodeArgs),
    /// Decode a herd of monkes back into the original file.
    Decode(DecodeArgs),
    /// Print metadata baked into a single monke.
    Inspect(InspectArgs),
    /// Report capacity-per-monke at various LSB depths.
    Capacity(CapacityArgs),
    /// P2P — connect to a remote PNGres and pull monkes off it.
    Remote(RemoteArgs),
    /// Launch the pngres-server HTTP daemon.
    #[cfg(feature = "serve")]
    Serve(ServeArgs),
}

pub fn dispatch(cmd: Command) -> Result<(), CliError> {
    match cmd {
        Command::Encode(args) => encode::run(args),
        Command::Decode(args) => decode::run(args),
        Command::Inspect(args) => inspect::run(args),
        Command::Capacity(args) => capacity::run(args),
        Command::Remote(args) => remote::run(args),
        #[cfg(feature = "serve")]
        Command::Serve(args) => serve::run(args),
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CliError {
    #[error(transparent)]
    Core(#[from] pngres_core::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("destination already exists: {0} (pass --force to overwrite)")]
    DestExists(std::path::PathBuf),
    #[error("input source is neither a directory nor a .zip: {0}")]
    UnknownSource(std::path::PathBuf),
    #[error("{0}")]
    Other(String),
}

impl CliError {
    pub fn exit_code(&self) -> ExitCode {
        use pngres_core::Error as E;
        match self {
            CliError::Core(E::HashMismatch { .. }) => ExitCode::from(5),
            CliError::Core(E::MissingShard { .. }) => ExitCode::from(4),
            CliError::Core(E::Corruption(_)) | CliError::Core(E::ChunkCrcMismatch) => {
                ExitCode::from(3)
            }
            CliError::DestExists(_) => ExitCode::from(6),
            _ => ExitCode::FAILURE,
        }
    }
}
