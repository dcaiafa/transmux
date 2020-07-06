use bytes::Buf;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum StreamDesc {
  Registration(RegistrationDesc),
  Metadata(MetadataDesc),
  Ac3(Ac3Desc),
  Eac3(Eac3Desc),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RegistrationDesc {
  pub format_id: u32,
}
const REGISTRATION_DESC_TAG: u8 = 5; // ISO/IEC 13818-1 Table 2-45

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MetadataDesc {
  pub app_format_id: Option<u32>,
}
const METADATA_DESC_TAG: u8 = 38; // ISO/IEC 13818-1 Table 2-45

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Ac3Desc;
const AC3_DESCRIPTOR_TAG: u8 = 106; // ETSI EN 300 468 Annex D (D.3)

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Eac3Desc;
const EAC3_DESCRIPTOR_TAG: u8 = 122; // ETSI EN 300 468 Annex D (D.5)

pub fn parse_stream_desc(tag: u8, buf: &[u8]) -> Option<StreamDesc> {
  let mut buf = buf;

  match tag {
    REGISTRATION_DESC_TAG => {
      if buf.len() < 4 {
        return None;
      }
      let format_id = buf.get_u32();
      Some(StreamDesc::Registration(RegistrationDesc { format_id }))
    }

    METADATA_DESC_TAG => {
      if buf.len() < 2 {
        return None;
      }
      let mut metadata_desc = MetadataDesc {
        app_format_id: None,
      };
      let metadata_app_format = buf.get_u16();
      if metadata_app_format == 0xffff {
        if buf.len() < 4 {
          return None;
        }
        metadata_desc.app_format_id = Some(buf.get_u32());
      }
      Some(StreamDesc::Metadata(metadata_desc))
    }

    AC3_DESCRIPTOR_TAG => Some(StreamDesc::Ac3(Ac3Desc {})),

    EAC3_DESCRIPTOR_TAG => Some(StreamDesc::Eac3(Eac3Desc {})),

    _ => None,
  }
}
