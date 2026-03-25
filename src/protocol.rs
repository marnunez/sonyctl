/// Sony headphones RFCOMM protocol implementation.
///
/// Packet format:
///   [START=0x3E] [escaped payload] [END=0x3C]
///
/// Payload (before escaping):
///   [data_type:1] [seq:1] [size:4 BE] [command_data:N] [checksum:1]
///
/// Sequence numbers alternate 0↔1 (1-bit toggle).
/// Client ACKs use (1 - device_msg_seq).
/// Client stores seq from device ACK for next outgoing command.

pub const START_MARKER: u8 = 0x3E;
pub const END_MARKER: u8 = 0x3C;
pub const ESCAPE_BYTE: u8 = 0x3D;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataType {
    Ack = 1,
    DataMdr = 12,   // COMMAND_1 in Gadgetbridge
    DataMdrNo2 = 14, // COMMAND_2 in Gadgetbridge
}

impl DataType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Ack),
            12 => Some(Self::DataMdr),
            14 => Some(Self::DataMdrNo2),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Message {
    pub data_type: DataType,
    pub seq: u8,
    pub payload: Vec<u8>,
}

pub fn escape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &b in data {
        match b {
            0x3C => { out.push(ESCAPE_BYTE); out.push(0x2C); }
            0x3D => { out.push(ESCAPE_BYTE); out.push(0x2D); }
            0x3E => { out.push(ESCAPE_BYTE); out.push(0x2E); }
            _ => out.push(b),
        }
    }
    out
}

pub fn unescape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == ESCAPE_BYTE && i + 1 < data.len() {
            let unescaped = match data[i + 1] {
                0x2C => 0x3C,
                0x2D => 0x3D,
                0x2E => 0x3E,
                other => other,
            };
            out.push(unescaped);
            i += 2;
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    out
}

pub fn checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

pub fn build_packet(dt: DataType, seq: u8, payload: &[u8]) -> Vec<u8> {
    let size = (payload.len() as u32).to_be_bytes();
    let mut raw = Vec::with_capacity(payload.len() + 7);
    raw.push(dt as u8);
    raw.push(seq);
    raw.extend_from_slice(&size);
    raw.extend_from_slice(payload);
    let chk = checksum(&raw);
    raw.push(chk);

    let mut pkt = Vec::with_capacity(raw.len() + 4);
    pkt.push(START_MARKER);
    pkt.extend_from_slice(&escape(&raw));
    pkt.push(END_MARKER);
    pkt
}

pub fn parse_packet(raw: &[u8]) -> Option<Message> {
    if raw.len() < 7 {
        return None;
    }
    let dt = DataType::from_u8(raw[0])?;
    let seq = raw[1];
    let size = u32::from_be_bytes([raw[2], raw[3], raw[4], raw[5]]) as usize;
    if raw.len() < 7 + size {
        return None;
    }
    let payload = raw[6..6 + size].to_vec();
    let chk = raw[6 + size];
    if chk != checksum(&raw[..6 + size]) {
        return None;
    }
    Some(Message { data_type: dt, seq, payload })
}

pub fn build_ack(device_seq: u8) -> Vec<u8> {
    build_packet(DataType::Ack, 1 - device_seq, &[])
}
