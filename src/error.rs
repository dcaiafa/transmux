use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum Error {}

pub type Result<T, E = Error> = std::result::Result<T, E>;
