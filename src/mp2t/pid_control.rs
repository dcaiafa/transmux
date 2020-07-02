use crate::context::Context;
use crate::mp2t::ts_parser::TsPacket;

pub enum Event<'a> {
  TsPacket(&'a TsPacket<'a>),
  Reset,
}

pub struct PidControl<H> {
  continuity_counter: Option<u8>,
  handler: H,
}

impl<H> PidControl<H>
where
  H: for<'a> FnMut(&mut Context, Event<'a>),
{
  pub fn new(handler: H) -> PidControl<H> {
    PidControl {
      continuity_counter: None,
      handler,
    }
  }

  pub fn parse_pkt<'a>(&mut self, ctx: &mut Context, pkt: &TsPacket<'a>) {
    // Implement continuity_counter semantics as specified in
    // ISO/IEC 13818-1 2.4.3.3.
    // N.B. the empty `Payload` indicates that the adaptation_field_control was
    // '10' or '00'.
    if pkt.payload.len() > 0 {
      if let Some(cc) = self.continuity_counter {
        let expected_cc = (cc + 1) % 16;
        if pkt.continuity_counter != expected_cc {
          if pkt.continuity_counter == cc {
            ctx.stats.duplicate_ts_packets += 1;
            return;
          }
          ctx.stats.continuity_counter_errors += 1;
          (self.handler)(ctx, Event::Reset {});
        }
      }
      self.continuity_counter = Some(pkt.continuity_counter);
    }

    (self.handler)(ctx, Event::TsPacket(pkt));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn new_handler<'a>(
    res: &'a mut Vec<String>,
  ) -> impl for<'b> FnMut(&mut Context, Event<'b>) + 'a {
    move |_: &mut Context, e: Event| match e {
      Event::TsPacket(&TsPacket {
        continuity_counter, ..
      }) => res.push(format!("{}", continuity_counter)),
      Event::Reset => res.push("R".to_string()),
    }
  }

  static PKT_PAYLOAD: &'static [u8] = &[1, 2, 3];

  fn pkt<'a>(continuity_counter: u8) -> TsPacket<'a> {
    TsPacket {
      continuity_counter,
      payload: PKT_PAYLOAD,
      ..Default::default()
    }
  }

  #[test]
  fn wrap() {
    let mut res = Vec::<String>::new();
    let handler = new_handler(&mut res);

    let mut ctx = Context::new();
    let mut pid_control = PidControl::new(handler);

    pid_control.parse_pkt(&mut ctx, &pkt(14));
    pid_control.parse_pkt(&mut ctx, &pkt(15));
    pid_control.parse_pkt(&mut ctx, &pkt(0));
    pid_control.parse_pkt(&mut ctx, &pkt(1));
    drop(pid_control);

    assert_eq!(res, vec!["14", "15", "0", "1"]);
    assert_eq!(ctx.stats.continuity_counter_errors, 0);
    assert_eq!(ctx.stats.duplicate_ts_packets, 0);
  }

  #[test]
  fn dup() {
    let mut res = Vec::<String>::new();
    let handler = new_handler(&mut res);

    let mut ctx = Context::new();
    let mut pid_control = PidControl::new(handler);

    pid_control.parse_pkt(&mut ctx, &pkt(14));
    pid_control.parse_pkt(&mut ctx, &pkt(15));
    pid_control.parse_pkt(&mut ctx, &pkt(15));
    pid_control.parse_pkt(&mut ctx, &pkt(0));
    pid_control.parse_pkt(&mut ctx, &pkt(1));
    drop(pid_control);

    assert_eq!(res, vec!["14", "15", "0", "1"]);
    assert_eq!(ctx.stats.continuity_counter_errors, 0);
    assert_eq!(ctx.stats.duplicate_ts_packets, 1);
  }

  #[test]
  fn discontinuity() {
    let mut res = Vec::<String>::new();
    let handler = new_handler(&mut res);

    let mut ctx = Context::new();
    let mut pid_control = PidControl::new(handler);

    pid_control.parse_pkt(&mut ctx, &pkt(5));
    pid_control.parse_pkt(&mut ctx, &pkt(6));
    pid_control.parse_pkt(&mut ctx, &pkt(3));
    pid_control.parse_pkt(&mut ctx, &pkt(4));
    pid_control.parse_pkt(&mut ctx, &pkt(5));
    drop(pid_control);

    assert_eq!(res, vec!["5", "6", "R", "3", "4", "5"]);
    assert_eq!(ctx.stats.continuity_counter_errors, 1);
    assert_eq!(ctx.stats.duplicate_ts_packets, 0);
  }
}
