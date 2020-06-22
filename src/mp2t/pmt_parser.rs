use crate::context::Context;
use crate::mp2t::desc::{self, StreamDesc};
use crate::mp2t::{Pmt, StreamInfo, StreamType};
use bytes::Buf;
use twiddle::Twiddle;

// ISO/IEC 13818-1 Table 2-45
const REGISTRATION_DESCRIPTOR: u8 = 5;
const METADATA_DESCRIPTOR: u8 = 38;

// User Private descriptors:
// AC-3 descriptor tag as defined in ETSI EN 300 468 Annex D (D.3)
const AC3_DESCRIPTOR: u8 = 106;
// Enhanced_AC-3 descriptor tag as defined in ETSI EN 300 468 Annex D (D.5)
const EAC3_DESCRIPTOR: u8 = 122;

pub struct PmtParser<H> {
  handler: H,
}

impl<H> PmtParser<H>
where
  H: FnMut(&mut Context, &Pmt),
{
  pub fn new(handler: H) -> PmtParser<H> {
    PmtParser { handler }
  }

  pub fn parse_psi(&mut self, ctx: &mut Context, psi: &[u8]) {
    if !self.parse(ctx, psi) {
      ctx.stats.invalid_pmt += 1;
    }
  }

  fn parse(&mut self, ctx: &mut Context, psi: &[u8]) -> bool {
    let mut buf = psi;
    if buf.len() < 9 {
      return false;
    }

    let program_number = buf.get_u16();
    let b = buf.get_u8();
    let version = b.bits(5..=1);
    let current_next = b.bit(0);
    let section = buf.get_u8();
    let last_section = buf.get_u8();
    let pcr_pid = buf.get_u16().bits(12..=0);

    if section != 0 || last_section != 0 {
      return false;
    }

    let mut pmt = Pmt {
      program_number,
      version,
      current_next,
      pcr_pid,
      streams: Vec::new(),
    };

    let program_info_len = buf.get_u16().bits(11..=0) as usize;
    if program_info_len > buf.len() {
      return false;
    }
    buf.advance(program_info_len);

    let mut index: usize = 0;
    while buf.len() >= 5 {
      let raw_stream_type = StreamType(buf.get_u8() as u32);
      let stream_type = raw_stream_type;
      let pid = buf.get_u16().bits(12..=0);
      let es_info_len = buf.get_u16().bits(11..=0) as usize;
      if es_info_len > buf.len() {
        return false;
      }

      // Parse stream descriptors.
      let mut es_info = &buf[..es_info_len];
      let mut descs = Vec::<StreamDesc>::new();
      while es_info.len() >= 2 {
        let desc_tag = es_info.get_u8();
        let desc_len = es_info.get_u8() as usize;
        if desc_len > es_info.len() {
          return false;
        }
        let desc_buf = &es_info[..desc_len];
        if let Some(desc) = desc::parse_stream_desc(desc_tag, desc_buf) {
          descs.push(desc);
        }
        es_info.advance(desc_len);
      }

      pmt.streams.push(StreamInfo {
        pid,
        stream_type,
        index,
        descs,
      });

      index += 1;
    }

    (self.handler)(ctx, &pmt);
    true
  }
}
