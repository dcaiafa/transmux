#[derive(Default)]
pub struct Stats {
  pub unsynchronized_bytes: u64,
  pub malformed_ts_packets: u64,
  pub invalid_psi: u64,
  pub psi_crc_errors: u64,
  pub skipped_unstarted_psi_pkts: u64,
}

pub struct Context {
  pub stats: Stats,
}

impl Context {
  pub fn new() -> Context {
    Context {
      stats: Default::default(),
    }
  }
}
