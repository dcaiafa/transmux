use std::fmt;

mod desc;
mod pat_parser;
mod pid_control;
mod pmt_parser;
mod psi_parser;
mod ts_parser;

pub mod demuxer;

pub use desc::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StreamType(u32);

impl fmt::Display for StreamType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", stream_type_str(self))
  }
}

impl fmt::Debug for StreamType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    (self as &dyn fmt::Display).fmt(f)
  }
}

macro_rules! stream_type {
  ( $( $n:ident = $v:expr ),* $(,)? ) => {
    $(
      const $n: StreamType = StreamType($v);
    )*
    fn stream_type_str(v: &StreamType) -> String {
      let name: &'static str = match *v {
        $(
          $n => stringify!($n),
        )*
        _ => "undefined",
      };
      format!("{} (0x{:x})", name, v.0)
    }
  };
}

stream_type![
  MPEG1_VIDEO = 0x01,
  MPEG2_VIDEO = 0x02,
  MPEG1_AUDIO = 0x03,
  MPEG2_AUDIO = 0x04,
  PES_PRIVATE_DATA = 0x06,
  ADTS_AAC = 0x0F,
  METADATA = 0x15,
  AVC = 0x1B,
  HEVC = 0x24,
  TEMI = 0x27,
  AC3 = 0x81,
  SCTE35 = 0x86,
  EAC3 = 0x87,
  ENCRYPTED_AC3 = 0xC1,
  ENCRYPTED_EAC3 = 0xC2,
  ENCRYPTED_ADTS_AAC = 0xCF,
  ENCRYPTED_AVC = 0xDB,
];

const FOURCC_AC_3: u32 = 0x41432d33; // "AC-3"
const FOURCC_EAC3: u32 = 0x45414333; // "EAC3"
const FOURCC_ID3: u32 = 0x49443320; // "ID3 "

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Pat {
  pub transport_stream_id: u16,
  pub version: u8,
  pub current_next: bool,
  pub section: u8,
  pub last_section: u8,
  pub network_pid: Option<u16>,
  pub programs: Vec<ProgramInfo>,
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct ProgramInfo {
  pub number: u16,
  pub pid: u16,
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Pmt {
  pub program_number: u16,
  pub version: u8,
  pub current_next: bool,
  pub pcr_pid: u16,
  pub streams: Vec<StreamInfo>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StreamInfo {
  pub pid: u16,
  pub stream_type: StreamType,
  pub index: usize,
  pub descs: Vec<StreamDesc>,
}
