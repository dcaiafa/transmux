use crate::context::Context;
use crate::internal::byte_queue::ByteQueue;
use bytes::Buf;
use twiddle::Twiddle;

const PACKET_SIZE: usize = 188;
const HEADER_SYNC_WORD: u8 = 0x47;

#[derive(Default)]
pub struct TsPacket<'a> {
  pub pos: i64,
  pub payload: &'a [u8],
  pub raw_data: &'a [u8],
  pub pid: u32,
  pub pcr: Option<u64>,
  pub continuity_counter: i32,
  pub payload_start: bool,
  pub discontinuity: bool,
  pub random_access: bool,
}

/// Implements a parser for MPEG-ts transport_packets as specified in
/// ISO/IEC 13818-1 2.4.3.2.
pub struct TsParser<H> {
  handler: H,
  byte_queue: ByteQueue,
  synchronized: bool,
}

impl<H> TsParser<H>
where
  H: FnMut(&mut Context, &TsPacket),
{
  pub fn new(handler: H) -> TsParser<H> {
    TsParser {
      handler: handler,
      byte_queue: ByteQueue::new(),
      synchronized: false,
    }
  }

  pub fn parse(&mut self, ctx: &mut Context, data: &[u8]) {
    self.byte_queue.write(data);
    while self.byte_queue.len() >= PACKET_SIZE {
      if !self.synchronized {
        self.synchronize(ctx);
        continue;
      }
      let packet = parse_packet(&self.byte_queue[..PACKET_SIZE]);
      match packet {
        Some(packet) => {
          (self.handler)(ctx, &packet);
          self.byte_queue.pop(PACKET_SIZE);
        }
        None => {
          // If we failed to parse a packet, we need to re-synchronize. Skip one
          // byte (so we don't try the same packet again), and synchronize()
          // will find the next packet.
          self.byte_queue.pop(1);
          self.synchronized = false;
          ctx.stats.malformed_ts_packets += 1;
          ctx.stats.unsynchronized_bytes += 1;
        }
      }
    }
  }

  fn synchronize(&mut self, ctx: &mut Context) {
    self.synchronized = false;
    let sync_idx = self.find_sync_word();
    match sync_idx {
      Some(idx) => {
        ctx.stats.unsynchronized_bytes += idx as u64;
        self.byte_queue.pop(idx);
        self.synchronized = true;
      }
      None => {
        ctx.stats.unsynchronized_bytes += self.byte_queue.len() as u64;
        self.byte_queue.pop_all();
      }
    }
  }

  fn find_sync_word(&self) -> Option<usize> {
    let buf = &self.byte_queue[..];
    for i in 0..buf.len() {
      let mut is_header = false;
      for j in 0..4 {
        let idx = i + j * PACKET_SIZE;
        if idx >= buf.len() {
          break;
        }
        if buf[idx] != HEADER_SYNC_WORD {
          is_header = false;
          break;
        }
        is_header = true;
      }
      if is_header {
        return Some(i);
      }
    }
    None
  }
}

fn parse_packet(data: &[u8]) -> Option<TsPacket> {
  debug_assert!(data.len() == PACKET_SIZE);

  // ISO/IEC 13818-1: 2.4.3.2 Transport Stream packet layer

  //  3          2          1          0
  // 10987654 32109876 54321098 76543210
  // aaaaaaaa bcdeeeee eeeeeeee ffgghhhh
  //
  // a: sync_word
  // b: transport_error
  // c: payload_unit_start
  // d: transport_priority
  // e: pid
  // f: transport_scrambling_control
  // g: adaptation_field_control
  // h: continuity_counter

  if data[0] != HEADER_SYNC_WORD {
    return None;
  }

  let mut buf = data;
  let mut packet: TsPacket = Default::default();
  let header = buf.get_u32();
  packet.raw_data = data;
  packet.payload_start = header.bit(22);
  packet.pid = header.bits(20..=8);
  let adaptation_field_control = header.bits(5..=4);
  packet.continuity_counter = header.bits(3..=0) as i32;

  let has_adaptation_field = adaptation_field_control & 0x2 != 0;
  let has_payload = adaptation_field_control & 0x1 != 0;
  if !has_adaptation_field {
    // There is no adaptation field. The remaining data is all payload.
    packet.payload = buf;
    return Some(packet);
  }

  // ISO/IEC 13818-1: 2.4.3.4 Adaptation field

  let adaptation_field_len = buf.get_u8() as usize;

  // If a payload is not specified, the adaptation field must take up the
  // entire packet. Conversely, if a payload is specified, the adaptation
  // field cannot take up the entire packet.
  if (!has_payload && adaptation_field_len != buf.len())
    || (has_payload && adaptation_field_len >= buf.len())
  {
    return None;
  }

  // adaptation_field_len = 0 is used to insert a single stuffing byte in
  // the adaptation field of a transport stream packet.
  if adaptation_field_len == 0 {
    packet.payload = buf;
    return Some(packet);
  }

  let mut adaptation_field = buf;

  let mut t = adaptation_field.get_u8() as u64;
  packet.discontinuity = t.bit(7);
  packet.random_access = t.bit(6);
  let pcr_flag = t.bit(4);

  if pcr_flag {
    t = adaptation_field.get_uint(6);
    let mut pcr: u64 = t.bits(47..=15) * 300;
    pcr += t.bits(8..=0);
    packet.pcr = Some(pcr);
  }

  // Skip the remaining adaptation field.
  buf.advance(adaptation_field_len);
  packet.payload = buf;

  Some(packet)
}

#[cfg(test)]
mod tests {
  use super::*;
  use mockall::automock;

  const PKT_NO_AF: &'static [u8] = &[
    0x47, 0x00, 0x65, 0x15, 0x9c, 0x04, 0x84, 0x4c, 0x16, 0x73, 0x53, 0x6e,
    0xb5, 0xf1, 0xd8, 0x55, 0x66, 0x62, 0xb8, 0xc7, 0x72, 0x31, 0xda, 0x0c,
    0x1a, 0xb2, 0x92, 0x28, 0x36, 0xd4, 0x10, 0xfb, 0x9c, 0x7e, 0xfa, 0xf7,
    0x13, 0xe1, 0xf6, 0x9f, 0xf9, 0x27, 0x39, 0x88, 0x90, 0x23, 0x25, 0x7c,
    0xcb, 0xe5, 0xbe, 0x1b, 0x57, 0xbc, 0xda, 0x1b, 0x98, 0xbb, 0xe1, 0xeb,
    0xcb, 0x23, 0xdc, 0x1f, 0x78, 0x9a, 0x45, 0x4c, 0x58, 0xd6, 0x4e, 0x1d,
    0x9b, 0xab, 0xe7, 0x0d, 0xe4, 0x68, 0x29, 0x58, 0x0d, 0x67, 0x1d, 0x5d,
    0xab, 0xd6, 0x5d, 0xe9, 0x1b, 0x3b, 0x1a, 0x5f, 0x0e, 0x4b, 0xed, 0x8e,
    0x41, 0xd8, 0xde, 0xef, 0x65, 0x5f, 0x70, 0x26, 0x90, 0x17, 0xab, 0x10,
    0x8a, 0xc4, 0xd4, 0xf1, 0x8e, 0x49, 0xce, 0x27, 0x28, 0xc2, 0x0f, 0xee,
    0xf6, 0xbb, 0x85, 0x15, 0x9a, 0x95, 0x79, 0x3d, 0x1d, 0x02, 0xb5, 0xdd,
    0x03, 0xc8, 0xec, 0x40, 0x44, 0xa8, 0x25, 0x17, 0x03, 0x17, 0xc9, 0x1d,
    0xce, 0x10, 0x59, 0x00, 0x9c, 0x99, 0xfa, 0x3d, 0xbd, 0xb1, 0x1b, 0x36,
    0xa6, 0x6c, 0x00, 0x00, 0x5e, 0x73, 0x8a, 0x28, 0x70, 0x41, 0x87, 0xec,
    0xa3, 0xa7, 0x0c, 0x0a, 0x36, 0xe7, 0x87, 0x7b, 0xcc, 0x64, 0x6d, 0x5a,
    0xf4, 0x10, 0xc6, 0xad, 0xe4, 0x92, 0x45, 0xa2,
  ];

  const PKT_TINY_AF: &'static [u8] = &[
    0x47, 0x40, 0x00, 0x30, 0x01, 0x00, 0x00, 0x00, 0xb0, 0x0d, 0x00, 0x01,
    0xc1, 0x00, 0x00, 0x00, 0x01, 0xe0, 0x64, 0x85, 0x41, 0x2f, 0xea, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
  ];

  const PKT_ZERO_AF: &'static [u8] = &[
    0x47, 0x40, 0x00, 0x30, 0x00, 0x62, 0xc7, 0x4b, 0xb0, 0x0d, 0x00, 0x01,
    0xb5, 0xf1, 0xd8, 0x55, 0x66, 0x62, 0xb8, 0xc7, 0x72, 0x31, 0xda, 0x0c,
    0x1a, 0xb2, 0x92, 0x28, 0x36, 0xd4, 0x10, 0xfb, 0x9c, 0x7e, 0xfa, 0xf7,
    0x13, 0xe1, 0xf6, 0x9f, 0xf9, 0x27, 0x39, 0x88, 0x90, 0x23, 0x25, 0x7c,
    0xcb, 0xe5, 0xbe, 0x1b, 0x57, 0xbc, 0xda, 0x1b, 0x98, 0xbb, 0xe1, 0xeb,
    0xcb, 0x23, 0xdc, 0x1f, 0x78, 0x9a, 0x45, 0x4c, 0x58, 0xd6, 0x4e, 0x1d,
    0x9b, 0xab, 0xe7, 0x0d, 0xe4, 0x68, 0x29, 0x58, 0x0d, 0x67, 0x1d, 0x5d,
    0xab, 0xd6, 0x5d, 0xe9, 0x1b, 0x3b, 0x1a, 0x5f, 0x0e, 0x4b, 0xed, 0x8e,
    0x41, 0xd8, 0xde, 0xef, 0x65, 0x5f, 0x70, 0x26, 0x90, 0x17, 0xab, 0x10,
    0x8a, 0xc4, 0xd4, 0xf1, 0x8e, 0x49, 0xce, 0x27, 0x28, 0xc2, 0x0f, 0xee,
    0xf6, 0xbb, 0x85, 0x15, 0x9a, 0x95, 0x79, 0x3d, 0x1d, 0x02, 0xb5, 0xdd,
    0x03, 0xc8, 0xec, 0x40, 0x44, 0xa8, 0x25, 0x17, 0x03, 0x17, 0xc9, 0x1d,
    0xce, 0x10, 0x59, 0x00, 0x9c, 0x99, 0xfa, 0x3d, 0xbd, 0xb1, 0x1b, 0x36,
    0xa6, 0x6c, 0x00, 0x00, 0x5e, 0x73, 0x8a, 0x28, 0x70, 0x41, 0x87, 0xec,
    0xa3, 0xa7, 0x0c, 0x0a, 0x36, 0xe7, 0x87, 0x7b, 0xcc, 0x64, 0x6d, 0x5a,
    0xf4, 0x10, 0xc6, 0xad, 0xe4, 0x92, 0x45, 0xa2,
  ];

  const PKT_AF_PCR: &'static [u8] = &[
    0x47, 0x40, 0x65, 0x30, 0x07, 0x50, 0xde, 0x36, 0xea, 0x29, 0x80, 0x00,
    0x00, 0x00, 0x01, 0xe0, 0x34, 0x08, 0x84, 0xc0, 0x0a, 0x3d, 0xf1, 0xb7,
    0xc0, 0x1d, 0x1d, 0xf1, 0xb7, 0xa8, 0xa7, 0x00, 0x00, 0x00, 0x01, 0x09,
    0x10, 0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x20, 0xac, 0xd9, 0x40,
    0xf0, 0x11, 0x7e, 0xe1, 0x00, 0x00, 0x03, 0x03, 0xe9, 0x00, 0x01, 0xd4,
    0xc0, 0x8f, 0x18, 0x31, 0x96, 0x00, 0x00, 0x00, 0x01, 0x68, 0xea, 0xef,
    0x2c, 0x00, 0x00, 0x01, 0x06, 0x05, 0xff, 0xff, 0xf0, 0xdc, 0x45, 0xe9,
    0xbd, 0xe6, 0xd9, 0x48, 0xb7, 0x96, 0x2c, 0xd8, 0x20, 0xd9, 0x23, 0xee,
    0xef, 0x78, 0x32, 0x36, 0x34, 0x20, 0x2d, 0x20, 0x63, 0x6f, 0x72, 0x65,
    0x20, 0x31, 0x35, 0x37, 0x20, 0x72, 0x32, 0x39, 0x34, 0x35, 0x20, 0x37,
    0x32, 0x64, 0x62, 0x34, 0x33, 0x37, 0x20, 0x2d, 0x20, 0x48, 0x2e, 0x32,
    0x36, 0x34, 0x2f, 0x4d, 0x50, 0x45, 0x47, 0x2d, 0x34, 0x20, 0x41, 0x56,
    0x43, 0x20, 0x63, 0x6f, 0x64, 0x65, 0x63, 0x20, 0x2d, 0x20, 0x43, 0x6f,
    0x70, 0x79, 0x6c, 0x65, 0x66, 0x74, 0x20, 0x32, 0x30, 0x30, 0x33, 0x2d,
    0x32, 0x30, 0x31, 0x38, 0x20, 0x2d, 0x20, 0x68, 0x74, 0x74, 0x70, 0x3a,
    0x2f, 0x2f, 0x77, 0x77, 0x77, 0x2e, 0x76, 0x69,
  ];

  const PKT_NO_PAYLOAD: &'static [u8] = &[
    0x47, 0x40, 0x65, 0x20, 0xB7, 0x50, 0xde, 0x36, 0xea, 0x29, 0x80, 0x00,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
  ];

  #[automock]
  trait Handler {
    fn on_pkt<'a>(&mut self, pkt: &TsPacket<'a>);
  }

  #[test]
  fn pkt_no_af() {
    let mut handler = MockHandler::new();

    handler
      .expect_on_pkt()
      .times(1)
      .withf(|pkt: &TsPacket| {
        pkt.pid == 0x65 && pkt.payload == &PKT_NO_AF[4..] && pkt.pcr.is_none()
      })
      .return_const(());

    let mut ctx = Context::new();
    let mut parser = TsParser::new(|_, pkt| handler.on_pkt(pkt));
    parser.parse(&mut ctx, PKT_NO_AF);

    assert_eq!(ctx.stats.unsynchronized_bytes, 0);
    assert_eq!(ctx.stats.malformed_ts_packets, 0);
  }

  #[test]
  fn pkt_tiny_af() {
    let mut handler = MockHandler::new();

    handler
      .expect_on_pkt()
      .times(1)
      .withf(|pkt: &TsPacket| {
        pkt.payload == &PKT_TINY_AF[6..] && pkt.pcr.is_none()
      })
      .return_const(());

    let mut ctx = Context::new();
    let mut parser = TsParser::new(|_, pkt| handler.on_pkt(pkt));
    parser.parse(&mut ctx, PKT_TINY_AF);

    assert_eq!(ctx.stats.unsynchronized_bytes, 0);
    assert_eq!(ctx.stats.malformed_ts_packets, 0);
  }

  #[test]
  fn pkt_af_pcr() {
    let mut handler = MockHandler::new();

    handler
      .expect_on_pkt()
      .times(1)
      .withf(|pkt: &TsPacket| {
        pkt.payload == &PKT_AF_PCR[12..] && pkt.pcr.unwrap() == 2236884504900
      })
      .return_const(());

    let mut ctx = Context::new();
    let mut parser = TsParser::new(|_, pkt| handler.on_pkt(pkt));
    parser.parse(&mut ctx, PKT_AF_PCR);

    assert_eq!(ctx.stats.unsynchronized_bytes, 0);
    assert_eq!(ctx.stats.malformed_ts_packets, 0);
  }

  #[test]
  fn pkt_zero_af() {
    let mut handler = MockHandler::new();

    handler
      .expect_on_pkt()
      .times(1)
      .withf(|pkt: &TsPacket| pkt.payload == &PKT_ZERO_AF[5..])
      .return_const(());

    let mut ctx = Context::new();
    let mut parser = TsParser::new(|_, pkt| handler.on_pkt(pkt));
    parser.parse(&mut ctx, PKT_ZERO_AF);

    assert_eq!(ctx.stats.unsynchronized_bytes, 0);
    assert_eq!(ctx.stats.malformed_ts_packets, 0);
  }

  #[test]
  fn pkt_no_payload() {
    let mut handler = MockHandler::new();

    handler
      .expect_on_pkt()
      .times(1)
      .withf(|pkt: &TsPacket| pkt.payload.len() == 0)
      .return_const(());

    let mut ctx = Context::new();
    let mut parser = TsParser::new(|_, pkt| handler.on_pkt(pkt));
    parser.parse(&mut ctx, PKT_NO_PAYLOAD);

    assert_eq!(ctx.stats.unsynchronized_bytes, 0);
    assert_eq!(ctx.stats.malformed_ts_packets, 0);
  }

  #[test]
  fn pkt_error_payload_indicated_but_not_present() {
    let mut handler = MockHandler::new();

    handler.expect_on_pkt().times(0).return_const(());

    let mut ctx = Context::new();
    let mut parser = TsParser::new(|_, pkt| handler.on_pkt(pkt));

    let mut data: Vec<u8> = PKT_NO_PAYLOAD.iter().cloned().collect();
    data[3] |= 0x10;
    parser.parse(&mut ctx, &data);

    assert_eq!(ctx.stats.malformed_ts_packets, 1);
  }

  #[test]
  fn sync_no_skip() {
    let mut handler = MockHandler::new();

    handler.expect_on_pkt().times(4).return_const(());

    let mut ctx = Context::new();
    let mut parser = TsParser::new(|_, pkt| handler.on_pkt(pkt));

    let mut data: Vec<u8> = PKT_AF_PCR.iter().cloned().collect();
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());

    parser.parse(&mut ctx, &data);

    assert_eq!(ctx.stats.unsynchronized_bytes, 0);
    assert_eq!(ctx.stats.malformed_ts_packets, 0);
  }

  #[test]
  fn sync_start() {
    let mut handler = MockHandler::new();

    handler.expect_on_pkt().times(3).return_const(());

    let mut ctx = Context::new();
    let mut parser = TsParser::new(|_, pkt| handler.on_pkt(pkt));

    let mut data: Vec<u8> = vec![0x1b, 0x47, 0xaa, 0x00];
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());

    parser.parse(&mut ctx, &data);

    assert_eq!(ctx.stats.unsynchronized_bytes, 4);
    assert_eq!(ctx.stats.malformed_ts_packets, 0);
  }

  #[test]
  fn sync_middle() {
    let mut handler = MockHandler::new();

    handler.expect_on_pkt().times(4).return_const(());

    let mut ctx = Context::new();
    let mut parser = TsParser::new(|_, pkt| handler.on_pkt(pkt));

    let mut data: Vec<u8> = PKT_AF_PCR.iter().cloned().collect();
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend([0x00u8, 0x47, 0x00].iter());
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());

    parser.parse(&mut ctx, &data);

    // The first two packets (+ the 3 garbage bytes) were skipped because the
    // parser needs consecutive 4 packets to synchronize.
    assert_eq!(ctx.stats.unsynchronized_bytes, 379);
    assert_eq!(ctx.stats.malformed_ts_packets, 0);
  }

  #[test]
  fn resync() {
    let mut handler = MockHandler::new();

    handler.expect_on_pkt().times(7).return_const(());

    let mut ctx = Context::new();
    let mut parser = TsParser::new(|_, pkt| handler.on_pkt(pkt));

    let mut data: Vec<u8> = PKT_AF_PCR.iter().cloned().collect();
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend([0x00u8, 0x47, 0x00].iter());
    data.extend(PKT_AF_PCR.iter().cloned());
    data.extend(PKT_AF_PCR.iter().cloned());

    parser.parse(&mut ctx, &data);

    assert_eq!(ctx.stats.unsynchronized_bytes, 3);
    assert_eq!(ctx.stats.malformed_ts_packets, 1);
  }
}
