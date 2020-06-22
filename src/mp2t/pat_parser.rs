use crate::context::Context;
use crate::mp2t::{Pat, ProgramInfo};
use bytes::Buf;
use twiddle::Twiddle;

pub struct PatParser<H> {
  pat_handler: H,
}

impl<H> PatParser<H>
where
  H: FnMut(&mut Context, &Pat),
{
  pub fn new(handler: H) -> PatParser<H> {
    PatParser {
      pat_handler: handler,
    }
  }

  pub fn parse_psi(&mut self, ctx: &mut Context, psi: &[u8]) {
    if !self.parse(ctx, psi) {
      ctx.stats.invalid_psi += 1;
    }
  }

  fn parse(&mut self, ctx: &mut Context, psi: &[u8]) -> bool {
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

    (self.pat_handler)(ctx, &pat);

    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::context::Context;
  use crate::mp2t::{Pat, ProgramInfo};
  use mockall::automock;
  use mockall::predicate::eq;

  #[automock]
  trait Handler {
    fn on_pat(&mut self, pat: &Pat);
  }

  static PAT: &'static [u8] = &[
    0x00, 0x01, 0xc1, 0x00, 0x00, 0x00, 0x00, 0xe0, 0xa, 0x00, 0x01, 0xe0,
    0x64, 0x04, 0xd2, 0xe3, 0xe9,
  ];

  #[test]
  fn basic() {
    let mut handler = MockHandler::new();

    handler
      .expect_on_pat()
      .with(eq(Pat {
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
      }))
      .return_const(())
      .times(1);

    let mut ctx = Context::new();
    let mut parser =
      PatParser::new(|_ctx: &mut Context, pat: &Pat| handler.on_pat(pat));
    parser.parse_psi(&mut ctx, PAT);
  }
}
