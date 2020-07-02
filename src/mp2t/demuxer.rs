use crate::context::Context;
use crate::mp2t::ts_parser::{TsPacket, TsParser};

use std::collections::hash_map::HashMap;

pub enum Event {
  ProgramSet,
  Program,
}

pub struct PesPacket {}

pub struct Demuxer {
  ts_parser: TsParser,
  pids: HashMap<u16, Box<dyn Fn(&mut Context, PesPacket)>>,
}

impl Demuxer {
  pub fn new() -> Demuxer {
    Demuxer {
      ts_parser: TsParser::new(),
      pids: HashMap::new(),
    }
  }

  pub fn parse(&mut self, ctx: &mut Context, data: &[u8]) {
    self.ts_parser.parse(ctx, data, |_ctx, _pkt| ());
  }

  fn on_pkt(&mut self, _ctx: &mut Context, _pkt: &TsPacket) {}
}
