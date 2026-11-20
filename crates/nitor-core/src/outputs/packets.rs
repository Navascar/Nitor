//! Пакети протоколів: Enttec DMX USB Pro, Art-Net (ArtDmx), sACN (E1.31).

use crate::DMX_CHANNELS;

/// Кадр «Output Only Send DMX» (label 6) для Enttec DMX USB Pro.
pub fn enttec_pro_packet(data: &[u8]) -> Vec<u8> {
    let data = &data[..data.len().min(DMX_CHANNELS)];
    let n = data.len() + 1; // + start code
    let mut p = Vec::with_capacity(n + 5);
    p.extend([0x7E, 6, (n & 0xFF) as u8, (n >> 8) as u8, 0]);
    p.extend_from_slice(data);
    p.push(0xE7);
    p
}

/// Пакет ArtDmx. `universe` — 15-бітна Port-Address (Net:SubNet:Universe).
pub fn artnet_packet(data: &[u8], universe: u16, sequence: u8) -> Vec<u8> {
    let mut payload = data[..data.len().min(DMX_CHANNELS)].to_vec();
    if payload.len() % 2 == 1 {
        payload.push(0); // довжина має бути парною
    }
    let mut p = Vec::with_capacity(18 + payload.len());
    p.extend_from_slice(b"Art-Net\0");
    p.extend(0x5000u16.to_le_bytes()); // OpCode ArtDmx
    p.extend(14u16.to_be_bytes()); // ProtVer
    p.extend([sequence, 0]); // Sequence, Physical
    p.extend([(universe & 0xFF) as u8, ((universe >> 8) & 0x7F) as u8]); // SubUni, Net
    p.extend((payload.len() as u16).to_be_bytes());
    p.extend(payload);
    p
}

/// Пакет даних E1.31 (sACN) з повним набором із 512 слотів.
pub fn sacn_packet(
    data: &[u8],
    universe: u16,
    sequence: u8,
    cid: &[u8; 16],
    source_name: &str,
    priority: u8,
) -> Vec<u8> {
    let mut slots = vec![0u8; DMX_CHANNELS + 1]; // start code + 512
    let n = data.len().min(DMX_CHANNELS);
    slots[1..=n].copy_from_slice(&data[..n]);
    let flen = |len: usize| (0x7000u16 | len as u16).to_be_bytes();

    let mut dmp = Vec::new();
    dmp.extend(flen(10 + slots.len()));
    dmp.extend([0x02, 0xa1]);
    dmp.extend(0u16.to_be_bytes());
    dmp.extend(1u16.to_be_bytes());
    dmp.extend((slots.len() as u16).to_be_bytes());
    dmp.extend(&slots);

    let mut name = source_name.as_bytes()[..source_name.len().min(63)].to_vec();
    name.resize(64, 0);
    let mut framing_body = Vec::new();
    framing_body.extend(2u32.to_be_bytes());
    framing_body.extend(name);
    framing_body.push(priority);
    framing_body.extend(0u16.to_be_bytes()); // synchronization address
    framing_body.extend([sequence, 0]); // sequence, options
    framing_body.extend(universe.to_be_bytes());
    let mut framing = Vec::new();
    framing.extend(flen(2 + framing_body.len() + dmp.len()));
    framing.extend(framing_body);
    framing.extend(dmp);

    let mut root_body = Vec::new();
    root_body.extend(4u32.to_be_bytes());
    root_body.extend(cid);
    let mut p = Vec::new();
    p.extend(0x0010u16.to_be_bytes());
    p.extend(0x0000u16.to_be_bytes());
    p.extend(b"ASC-E1.17\0\0\0");
    p.extend(flen(2 + root_body.len() + framing.len()));
    p.extend(root_body);
    p.extend(framing);
    p
}

pub fn sacn_multicast_address(universe: u16) -> std::net::Ipv4Addr {
    std::net::Ipv4Addr::new(239, 255, (universe >> 8) as u8, (universe & 0xFF) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enttec() {
        let data: Vec<u8> = (0..=255u8).chain(0..=255u8).collect();
        let p = enttec_pro_packet(&data);
        assert_eq!(&p[..4], &[0x7E, 6, (513 & 0xFF) as u8, (513 >> 8) as u8]);
        assert_eq!(p[4], 0);
        assert_eq!(&p[5..p.len() - 1], &data[..]);
        assert_eq!(*p.last().unwrap(), 0xE7);
        assert_eq!(p.len(), 518);
    }

    #[test]
    fn artnet() {
        let p = artnet_packet(&[1, 2, 3], 0x123, 7);
        assert_eq!(&p[..8], b"Art-Net\0");
        assert_eq!(u16::from_le_bytes([p[8], p[9]]), 0x5000);
        assert_eq!(&p[10..12], &[0, 14]);
        assert_eq!(p[12], 7);
        assert_eq!((p[14], p[15]), (0x23, 0x01));
        assert_eq!(u16::from_be_bytes([p[16], p[17]]), 4);
        assert_eq!(&p[18..], &[1, 2, 3, 0]);
    }

    #[test]
    fn sacn_layout() {
        let cid: [u8; 16] = std::array::from_fn(|i| i as u8);
        let p = sacn_packet(&[9; 512], 5, 3, &cid, "DMX RGB Controller", 100);
        let be = |i: usize| u16::from_be_bytes([p[i], p[i + 1]]);
        assert_eq!(p.len(), 638);
        assert_eq!(&p[4..16], b"ASC-E1.17\0\0\0");
        assert_eq!(be(16), 0x7000 | 622);
        assert_eq!(&p[22..38], &cid);
        assert_eq!(be(38), 0x7000 | 600);
        assert_eq!(p[111], 3);
        assert_eq!(be(113), 5);
        assert_eq!(be(115), 0x7000 | 523);
        assert_eq!(be(123), 513);
        assert_eq!(p[125], 0);
        assert!(p[126..].iter().all(|&b| b == 9));
        assert_eq!(sacn_multicast_address(300).to_string(), "239.255.1.44");
    }
}
