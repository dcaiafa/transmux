use crate::mp2t::demuxer::{Context, Event};
use crate::mp2t::psi_parser::PsiHandler;
use crate::mp2t::{Pat, ProgramInfo};
use bytes::Buf;
use twiddle::Twiddle;

pub struct PatParser {
  current: Option<Pat>,
}

impl PatParser {
  pub fn new() -> PatParser {
    PatParser { current: None }
  }

  fn parse_psi(&mut self, ctx: &mut Context, psi: &[u8]) -> bool {
    let mut buf = psi;

    if buf.len() < 5 {
      return false;
    }

    let mut pat: Pat = Default::default();

    pat.transport_stream_id = buf.get_u16();
    let b = buf.get_u8();
    pat.version = b.bits(5..=1);
    pat.current_next = b.bit(0);
    pat.section = buf.get_u8();
    pat.last_section = buf.get_u8();

    while buf.len() >= 4 {
      let program_number = buf.get_u16();
      let pid = buf.get_u16().bits(12..=0);

      if program_number == 0 {
        pat.network_pid = Some(pid);
      } else {
        pat.programs.push(ProgramInfo {
          number: program_number,
          pid: pid,
        });
      }
    }

    let changed = match self.current {
      Some(ref current) => pat != *current,
      None => true,
    };

    if changed {
      ctx.events.push_back(Event::Pat(pat.clone()));
      self.current = Some(pat);
    }

    true
  }
}

impl PsiHandler for PatParser {
  const TABLE_ID: u8 = 0; // From ISO/IEC 13818-1: Table 2-31

  fn on_psi(&mut self, ctx: &mut Context, psi: &[u8]) {
    if !self.parse_psi(ctx, psi) {
      ctx.stats.invalid_psi += 1;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  static PAT: &'static [u8] = &[
    0x00, 0x01, 0xc1, 0x00, 0x00, 0x00, 0x00, 0xe0, 0xa, 0x00, 0x01, 0xe0,
    0x64, 0x04, 0xd2, 0xe3, 0xe9,
  ];

  macro_rules! assert_pattern {
    ( $input:expr, $pat:pat, $then:expr ) => {
      match $input {
        $pat => $then,
        _ => panic!("not a match"),
      }
    };
  }

  #[test]
  fn basic() {
    let mut ctx = Context::new();
    let mut parser = PatParser::new();
    parser.parse_psi(&mut ctx, PAT);
    parser.parse_psi(&mut ctx, PAT);

    assert_eq!(ctx.events.len(), 1);
    assert_pattern!(
      ctx.events[0],
      Event::Pat(ref pat),
      assert_eq!(
        pat,
        &Pat {
          transport_stream_id: 1,
          version: 0,
          current_next: true,
          section: 0,
          last_section: 0,
          network_pid: Some(10),
          programs: vec![
            ProgramInfo {
              number: 1,
              pid: 100,
            },
            ProgramInfo {
              number: 1234,
              pid: 1001,
            },
          ],
        }
      )
    );
  }
}
