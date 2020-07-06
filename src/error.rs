use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum Error {
  #[snafu(display("Invalid program number"))]
  InvalidProgramNumber,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
