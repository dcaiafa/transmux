use crate::mp2t::pat_parser::PatParser;
use crate::mp2t::pmt_parser::PmtParser;
use crate::mp2t::psi_parser::PsiParser;
use crate::mp2t::ts_parser::{TsHandler, TsPacket, TsParser};
use crate::mp2t::{Pat, Pmt, ProgramInfo};
use crate::stats::Stats;
use crate::{Error, Result};
use std::collections::hash_map::HashMap;
use std::collections::VecDeque;
use std::io;
use std::io::Read;

#[derive(Default, Debug, Clone)]
pub struct Program {
  pub program_info: ProgramInfo,
  pub pmt: Option<Pmt>,
  pub enabled: bool,
}

pub struct Context {
  pub stats: Stats,
  pub events: VecDeque<Event>,
}

impl Context {
  pub fn new() -> Context {
    Context {
      stats: Default::default(),
      events: VecDeque::new(),
    }
  }
}

#[derive(Debug)]
pub enum Event {
  Pat { new: Pat, old: Option<Pat> },
  Pmt { new: Pmt, old: Option<Pmt> },
  Pes,
}

pub struct PesPacket {}

pub struct Demuxer {
  ctx: Context,
  ts_parser: TsParser<Demult>,
  buf: [u8; 10240],
}

impl Demuxer {
  pub fn new() -> Demuxer {
    Demuxer {
      ctx: Context::new(),
      ts_parser: TsParser::new(Demult::new()),
      buf: [0; 10240],
    }
  }

  pub fn parse<'a, 'b>(
    &'a mut self,
    input: &'b mut dyn Read,
  ) -> io::Result<Option<Event>> {
    loop {
      self.ts_parser.parse(&mut self.ctx);
      match self.ctx.events.pop_front() {
        Some(e) => {
          match e {
            Event::Pat { new: ref pat, .. } => {
              self.ts_parser.mut_handler().on_pat(pat)
            }
            _ => (),
          }
          return Ok(Some(e));
        }
        None => {
          let n = input.read(&mut self.buf)?;
          if n == 0 {
            return Ok(None);
          }
          self.ts_parser.push(&self.buf[..n]);
        }
      }
    }
  }

  pub fn programs<'a>(&'a self) -> impl Iterator<Item = &'a Program> {
    self.ts_parser.handler().programs()
  }

  pub fn enable_program(&mut self, program_number: u16) -> Result<()> {
    self.ts_parser.mut_handler().enable_program(program_number)
  }
}

struct Demult {
  pids: HashMap<u16, Box<dyn TsHandler>>,
  programs: HashMap<u16, Program>,
}

impl Demult {
  pub fn new() -> Demult {
    let mut d = Demult {
      pids: HashMap::new(),
      programs: HashMap::new(),
    };
    d.pids.insert(0, Box::new(PsiParser::new(PatParser::new())));
    return d;
  }

  pub fn on_pat(&mut self, pat: &Pat) {
    let valid_programs: HashMap<u16, &ProgramInfo> =
      pat.programs.iter().map(|p| (p.number, p)).collect();

    // Compile list of programs currently tracked that were invalidated by this
    // new PAT. This includes programs not in the current PAT, or programs whose
    // program_pid changed.
    let dead_program_nums: Vec<u16> = self
      .programs
      .values()
      .filter(|existing_prog| {
        match valid_programs.get(&existing_prog.program_info.number) {
          Some(valid_prog) => existing_prog.program_info.pid != valid_prog.pid,
          None => false,
        }
      })
      .map(|prog| prog.program_info.number)
      .collect();

    for dead_program_num in dead_program_nums {
      // Remove all pid mappings associated with the dead program, including the
      // PMT's pid.
      let program_pid = self.programs[&dead_program_num].program_info.pid;
      self.pids.remove(&program_pid);
      if let Some(ref pmt) = self.programs[&dead_program_num].pmt {
        for ref stream in &pmt.streams {
          self.pids.remove(&stream.pid);
        }
      }

      // Stop tracking the program.
      self.programs.remove(&dead_program_num);
    }

    let mut new_programs: Vec<Program> = pat
      .programs
      .iter()
      .filter(|program_info| !self.programs.contains_key(&program_info.number))
      .map(|program_info| Program {
        program_info: program_info.clone(),
        pmt: None,
        enabled: false,
      })
      .collect();

    for prog in new_programs.drain(..) {
      self.programs.insert(prog.program_info.number, prog);
    }
  }

  pub fn programs<'a>(&'a self) -> impl Iterator<Item = &'a Program> {
    self.programs.values()
  }

  pub fn enable_program(&mut self, program_number: u16) -> Result<()> {
    match self.programs.get_mut(&program_number) {
      Some(ref mut prog) => {
        if !prog.enabled {
          prog.enabled = true;
          println!("Enabling program {:?}", prog);
          self.pids.insert(
            prog.program_info.pid,
            Box::new(PsiParser::new(PmtParser::new())),
          );
        }
        Ok(())
      }
      None => Err(Error::InvalidProgramNumber),
    }
  }
}

impl TsHandler for Demult {
  fn on_pkt(&mut self, ctx: &mut Context, pkt: &TsPacket) {
    match self.pids.get_mut(&pkt.pid) {
      Some(handler) => handler.on_pkt(ctx, pkt),
      None => ctx.stats.ignored_ts_packets += 1,
    }
  }
}
