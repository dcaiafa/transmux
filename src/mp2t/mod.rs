mod demuxer;
mod ts_parser;

#[derive(Debug)]
pub enum StreamType {
    Mpeg1Video = 0x01,
    Mpeg2Video = 0x02,
    Mpeg1Audio = 0x03,
    Mpeg2Audio = 0x04,
    PesPrivateData = 0x06,
    AdtsAac = 0x0F,
    Metadata = 0x15,
    Avc = 0x1B,
    Hevc = 0x24,
    Temi = 0x27,
    Ac3 = 0x81,
    Scte35 = 0x86,
    Eac3 = 0x87,
    EncryptedAc3 = 0xC1,
    EncryptedEac3 = 0xC2,
    EncryptedAdtsAac = 0xCF,
    EncryptedAvc = 0xDB,
}
