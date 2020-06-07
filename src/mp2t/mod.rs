mod demuxer;
mod pat_parser;
mod psi_parser;
mod ts_parser;

#[derive(Debug, Clone, Copy)]
pub enum StreamType {
  Mpeg1Video = 0x01,
  Mpeg2Video = 0x02,
  Mpeg1Audio = 0x03,
  Mpeg2Audio = 0x04,
  PesPrivateData = 0x06,
  AdtsAac = 0x0F,
  Metadata = 0x15,
  Avc = 0x1B,
  Hevc = 0x24,
  Temi = 0x27,
  Ac3 = 0x81,
  Scte35 = 0x86,
  Eac3 = 0x87,
  EncryptedAc3 = 0xC1,
  EncryptedEac3 = 0xC2,
  EncryptedAdtsAac = 0xCF,
  EncryptedAvc = 0xDB,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Pat {
  pub transport_stream_id: u16,
  pub version: u8,
  pub current_next: bool,
  pub section: u8,
  pub last_section: u8,
  pub network_pid: Option<u32>,
  pub programs: Vec<ProgramInfo>,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct ProgramInfo {
  pub number: u32,
  pub pid: u32,
}
