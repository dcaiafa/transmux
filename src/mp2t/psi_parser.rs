use crate::context::Context;
use crate::crc;
use crate::mp2t::ts_parser::TsPacket;
use bytes::Buf;

const MAX_SECTION_LEN: usize = 1021;

pub struct PsiParser<H> {
  table_id: u8,
  psi_handler: H,
  data: Vec<u8>,
  started: bool,
}

impl<H> PsiParser<H>
where
  H: FnMut(&mut Context, &[u8]),
{
  pub fn new(table_id: u8, psi_handler: H) -> PsiParser<H> {
    PsiParser {
      table_id,
      psi_handler,
      data: Vec::new(),
      started: false,
    }
  }

  pub fn parse_pkt<'p>(&mut self, ctx: &mut Context, pkt: &TsPacket<'p>) {
    if !self.parse(ctx, pkt) {
      ctx.stats.invalid_psi += 1;
      self.data.clear();
      self.started = false;
    }
  }

  fn parse<'p>(&mut self, ctx: &mut Context, pkt: &TsPacket<'p>) -> bool {
    if !self.started && !pkt.payload_start {
      ctx.stats.skipped_unstarted_psi_pkts += 1;

      // This is not an error: it is likely that we started the stream in the
      // middle of a psi.
      return true;
    }

    let mut pkt_data = pkt.payload;

    if pkt.payload_start {
      self.data.clear();
      self.started = true;

      if pkt_data.len() < 1 {
        return false;
      }

      let pointer_field = pkt_data.get_u8() as usize;
      if pkt_data.len() < pointer_field {
        return false;
      }

      // The pointer_field points to where the real data is in the packet, i.e.
      // it's the number of bytes to discard.
      pkt_data.advance(pointer_field);
    }

    self.data.extend_from_slice(pkt_data);

    let mut psi = &self.data[..];

    if psi.len() < 3 {
      // Not enough data to start parsing, yet.
      return true;
    }

    let table_id = psi[0];
    if table_id != self.table_id {
      return false;
    }

    let section_len = ((&psi[1..3]).get_u16() & 0xfff) as usize;
    if section_len > MAX_SECTION_LEN {
      return false;
    }

    let psi_len = section_len + 3;

    if psi.len() < psi_len {
      // Wait for the rest of the PSI.
      return true;
    } else if psi.len() > psi_len {
      psi = &psi[..psi_len];
    }

    let crc_sum = crc::mpeg2(psi);
    if crc_sum != 0 {
      ctx.stats.psi_crc_errors += 1;
      return false;
    }

    // Send to the handler the section data (starting after section_length)
    // minus the CRC.
    (self.psi_handler)(ctx, &psi[3..psi.len() - 4]);

    self.data.clear();
    self.started = false;

    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use mockall::automock;
  use mockall::predicate::eq;

  #[automock]
  trait Handler {
    fn on_psi(&mut self, psi: &[u8]);
  }

  const PSI: &'static [u8] = &[
    0x05, // pointer_field
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // skipped by pointer_field
    0x02, 0xB0, 0x0B, // table_id + section_length
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, // psi specific
    0x25, 0x1c, 0xd6, 0x79, // crc
    0xFF, 0xFF, 0xFF, 0xFF, // padding
  ];

  #[test]
  fn simple() {
    let mut handler = MockHandler::new();
    handler
      .expect_on_psi()
      .with(eq(&PSI[9..16]))
      .times(1)
      .return_const(());

    let mut ctx = Context::new();
    let mut parser = PsiParser::new(0x2, |_, psi| handler.on_psi(psi));

    let pkt = &TsPacket {
      payload: PSI,
      payload_start: true,
      ..Default::default()
    };
    parser.parse_pkt(&mut ctx, &pkt);
  }

  #[test]
  fn multiple_packets() {
    let mut handler = MockHandler::new();
    handler
      .expect_on_psi()
      .with(eq(&PSI[9..16]))
      .times(1)
      .return_const(());

    let mut ctx = Context::new();
    let mut parser = PsiParser::new(0x2, |_, psi| handler.on_psi(psi));

    parser.parse_pkt(
      &mut ctx,
      &TsPacket {
        payload: &PSI[0..8],
        payload_start: true,
        ..Default::default()
      },
    );
    parser.parse_pkt(
      &mut ctx,
      &TsPacket {
        payload: &PSI[8..13],
        payload_start: false,
        ..Default::default()
      },
    );
    parser.parse_pkt(
      &mut ctx,
      &TsPacket {
        payload: &PSI[13..],
        payload_start: false,
        ..Default::default()
      },
    );
  }

  #[test]
  fn not_started_before() {
    let mut handler = MockHandler::new();
    handler
      .expect_on_psi()
      .with(eq(&PSI[9..16]))
      .times(1)
      .return_const(());

    let mut ctx = Context::new();
    let mut parser = PsiParser::new(0x2, |_, psi| handler.on_psi(psi));

    parser.parse_pkt(
      &mut ctx,
      &TsPacket {
        payload: &[0xFF, 0xFF, 0xFF],
        payload_start: false,
        ..Default::default()
      },
    );
    parser.parse_pkt(
      &mut ctx,
      &TsPacket {
        payload: PSI,
        payload_start: true,
        ..Default::default()
      },
    );
  }

  #[test]
  fn not_started_middle() {
    let mut handler = MockHandler::new();
    handler
      .expect_on_psi()
      .with(eq(&PSI[9..16]))
      .times(2)
      .return_const(());

    let mut ctx = Context::new();
    let mut parser = PsiParser::new(0x2, |_, psi| handler.on_psi(psi));

    parser.parse_pkt(
      &mut ctx,
      &TsPacket {
        payload: PSI,
        payload_start: true,
        ..Default::default()
      },
    );
    parser.parse_pkt(
      &mut ctx,
      &TsPacket {
        payload: &[0xFF, 0xFF, 0xFF],
        payload_start: false,
        ..Default::default()
      },
    );
    parser.parse_pkt(
      &mut ctx,
      &TsPacket {
        payload: PSI,
        payload_start: true,
        ..Default::default()
      },
    );
  }
}
