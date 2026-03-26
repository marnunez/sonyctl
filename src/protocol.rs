//! Sony headphones RFCOMM protocol implementation.
//!
//! Packet format:
//!   `[START=0x3E] [escaped payload] [END=0x3C]`
//!
//! Payload (before escaping):
//!   `[data_type:1] [seq:1] [size:4 BE] [command_data:N] [checksum:1]`
//!
//! Sequence numbers alternate 0↔1 (1-bit toggle).
//! Client ACKs use `1 - device_msg_seq`.
//! Client stores seq from device ACK for next outgoing command.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_unescape_roundtrip() {
        // All three special bytes present
        let data = vec![0x00, 0x3C, 0xFF, 0x3D, 0x42, 0x3E, 0x01];
        assert_eq!(unescape(&escape(&data)), data);
    }

    #[test]
    fn escape_unescape_no_special() {
        let data = vec![0x00, 0x01, 0x02, 0xFF];
        let escaped = escape(&data);
        assert_eq!(escaped, data); // nothing to escape
        assert_eq!(unescape(&escaped), data);
    }

    #[test]
    fn escape_all_special() {
        let data = vec![0x3C, 0x3D, 0x3E];
        let escaped = escape(&data);
        assert_eq!(escaped, vec![0x3D, 0x2C, 0x3D, 0x2D, 0x3D, 0x2E]);
        assert_eq!(unescape(&escaped), data);
    }

    #[test]
    fn escape_empty() {
        assert_eq!(escape(&[]), Vec::<u8>::new());
        assert_eq!(unescape(&[]), Vec::<u8>::new());
    }

    #[test]
    fn checksum_basic() {
        assert_eq!(checksum(&[]), 0);
        assert_eq!(checksum(&[0x10, 0x20, 0x30]), 0x60);
        // Wrapping: 0xFF + 0x02 = 0x01
        assert_eq!(checksum(&[0xFF, 0x02]), 0x01);
    }

    #[test]
    fn build_parse_roundtrip() {
        let payload = vec![0x22, 0x00]; // e.g. get_battery
        let pkt = build_packet(DataType::DataMdr, 0, &payload);

        // Packet must be framed with START/END markers
        assert_eq!(pkt[0], START_MARKER);
        assert_eq!(*pkt.last().unwrap(), END_MARKER);

        // Extract and parse inner content
        let inner = unescape(&pkt[1..pkt.len() - 1]);
        let msg = parse_packet(&inner).expect("should parse");
        assert_eq!(msg.data_type, DataType::DataMdr);
        assert_eq!(msg.seq, 0);
        assert_eq!(msg.payload, payload);
    }

    #[test]
    fn build_parse_roundtrip_with_escaping() {
        // Payload containing bytes that need escaping
        let payload = vec![0x3C, 0x3D, 0x3E, 0x00, 0xFF];
        let pkt = build_packet(DataType::DataMdrNo2, 1, &payload);

        let inner = unescape(&pkt[1..pkt.len() - 1]);
        let msg = parse_packet(&inner).expect("should parse");
        assert_eq!(msg.data_type, DataType::DataMdrNo2);
        assert_eq!(msg.seq, 1);
        assert_eq!(msg.payload, payload);
    }

    #[test]
    fn build_parse_empty_payload() {
        let pkt = build_packet(DataType::Ack, 0, &[]);
        let inner = unescape(&pkt[1..pkt.len() - 1]);
        let msg = parse_packet(&inner).expect("should parse");
        assert_eq!(msg.data_type, DataType::Ack);
        assert_eq!(msg.payload, Vec::<u8>::new());
    }

    #[test]
    fn parse_rejects_bad_checksum() {
        let pkt = build_packet(DataType::DataMdr, 0, &[0x22, 0x00]);
        let mut inner = unescape(&pkt[1..pkt.len() - 1]);
        // Corrupt the checksum (last byte)
        *inner.last_mut().unwrap() ^= 0xFF;
        assert!(parse_packet(&inner).is_none());
    }

    #[test]
    fn parse_rejects_truncated() {
        assert!(parse_packet(&[]).is_none());
        assert!(parse_packet(&[0x0C, 0x00, 0x00]).is_none());
    }

    #[test]
    fn parse_rejects_unknown_data_type() {
        // Valid structure but unknown data_type byte
        let mut raw = vec![0xFF, 0x00, 0x00, 0x00, 0x00, 0x01, 0x42];
        raw.push(checksum(&raw));
        assert!(parse_packet(&raw).is_none());
    }

    #[test]
    fn build_ack_toggles_seq() {
        let ack0 = build_ack(0);
        let inner0 = unescape(&ack0[1..ack0.len() - 1]);
        let msg0 = parse_packet(&inner0).unwrap();
        assert_eq!(msg0.seq, 1); // device sent 0, we ACK with 1

        let ack1 = build_ack(1);
        let inner1 = unescape(&ack1[1..ack1.len() - 1]);
        let msg1 = parse_packet(&inner1).unwrap();
        assert_eq!(msg1.seq, 0); // device sent 1, we ACK with 0
    }

    #[test]
    fn data_type_from_u8() {
        assert_eq!(DataType::from_u8(1), Some(DataType::Ack));
        assert_eq!(DataType::from_u8(12), Some(DataType::DataMdr));
        assert_eq!(DataType::from_u8(14), Some(DataType::DataMdrNo2));
        assert_eq!(DataType::from_u8(0), None);
        assert_eq!(DataType::from_u8(255), None);
    }
}
