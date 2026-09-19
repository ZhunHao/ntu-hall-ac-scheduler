use ac_scheduler::ir::{
    DaikinCommand, DaikinPacket, OperationMode, BIT_MARK_US, DAIKIN_STATE_LENGTH, GAP_SPACE_US,
    HDR_MARK_US, HDR_SPACE_US, ONE_SPACE_US, ZERO_SPACE_US,
};

#[test]
fn test_daikin_packet_defaults_and_length() {
    let packet = DaikinPacket::new();
    assert_eq!(packet.raw_bytes().len(), DAIKIN_STATE_LENGTH);
    assert_eq!(packet.raw_bytes().len(), 35);
    assert!(packet.verify_checksums());
}

#[test]
fn test_daikin_section_headers() {
    let packet = DaikinPacket::new();
    let bytes = packet.raw_bytes();

    // Section 1 header: 11 DA 27 00 C5
    assert_eq!(bytes[0], 0x11);
    assert_eq!(bytes[1], 0xDA);
    assert_eq!(bytes[2], 0x27);
    assert_eq!(bytes[4], 0xC5);

    // Section 2 header: 11 DA 27 00 42
    assert_eq!(bytes[8], 0x11);
    assert_eq!(bytes[9], 0xDA);
    assert_eq!(bytes[10], 0x27);
    assert_eq!(bytes[12], 0x42);

    // Section 3 header: 11 DA 27
    assert_eq!(bytes[16], 0x11);
    assert_eq!(bytes[17], 0xDA);
    assert_eq!(bytes[18], 0x27);
}

#[test]
fn test_daikin_on_16_command() {
    let cmd = DaikinCommand::cool_at(16);
    let packet = DaikinPacket::from_command(&cmd);
    let bytes = packet.raw_bytes();

    assert!(packet.verify_checksums());
    // Power bit is set (bit 0 of byte 21)
    assert_eq!(bytes[21] & 0x01, 0x01);
    // Cool mode is set (bits 4..6 of byte 21 = 0b011)
    assert_eq!((bytes[21] >> 4) & 0x07, OperationMode::Cool as u8);
    // Temp byte is 16 * 2 = 32 (0x20)
    assert_eq!(bytes[22], 32);
    assert_eq!(packet.get_temp(), 16);
}

#[test]
fn test_daikin_on_25_command() {
    let cmd = DaikinCommand::cool_at(25);
    let packet = DaikinPacket::from_command(&cmd);
    let bytes = packet.raw_bytes();

    assert!(packet.verify_checksums());
    assert_eq!(bytes[21] & 0x01, 0x01);
    // Temp byte is 25 * 2 = 50 (0x32)
    assert_eq!(bytes[22], 50);
    assert_eq!(packet.get_temp(), 25);
}

#[test]
fn test_daikin_off_command() {
    let cmd = DaikinCommand::off();
    let packet = DaikinPacket::from_command(&cmd);
    let bytes = packet.raw_bytes();

    assert!(packet.verify_checksums());
    // Power bit is 0
    assert_eq!(bytes[21] & 0x01, 0x00);
}

/// Footer that IRremoteESP8266's `IRsend::sendDaikin` emits after every segment:
/// `mark(kDaikinBitMark)` then `space(kDaikinZeroSpace + kDaikinGap)`.
const FOOTER: (u32, u32) = (BIT_MARK_US, ZERO_SPACE_US + GAP_SPACE_US);

/// Decodes `n_bytes` LSB-first data bytes starting at `pulses[start]`.
fn decode_bytes(pulses: &[(u32, u32)], start: usize, n_bytes: usize) -> Vec<u8> {
    (0..n_bytes)
        .map(|byte| {
            (0..8).fold(0u8, |acc, bit| {
                let (mark, space) = pulses[start + byte * 8 + bit];
                assert_eq!(mark, BIT_MARK_US, "data bit mark");
                match space {
                    ONE_SPACE_US => acc | (1 << bit),
                    ZERO_SPACE_US => acc,
                    other => panic!("data bit space {other} µs is neither 0 nor 1"),
                }
            })
        })
        .collect()
}

#[test]
fn test_pulse_framing_matches_irremoteesp8266() {
    let packet = DaikinPacket::from_command(&DaikinCommand::cool_at(16));
    let pulses = packet.to_pulses();

    // Leader: 5 zero bits + footer
    // Sections 1, 2, 3: header + 8/8/19 bytes of data + footer
    // Total = (5 + 1) + (1 + 64 + 1) + (1 + 64 + 1) + (1 + 152 + 1) = 292 mark/space pairs
    assert_eq!(pulses.len(), 292);

    assert!(pulses[0..5]
        .iter()
        .all(|&p| p == (BIT_MARK_US, ZERO_SPACE_US)));
    assert_eq!(pulses[5], FOOTER);

    // Every section: header, data bits that decode back to the packet bytes, then a footer mark
    let raw = packet.raw_bytes();
    let mut idx = 6;
    for (start, len) in [(0, 8), (8, 8), (16, 19)] {
        assert_eq!(pulses[idx], (HDR_MARK_US, HDR_SPACE_US), "section header");
        assert_eq!(decode_bytes(&pulses, idx + 1, len), raw[start..start + len]);
        idx += 1 + len * 8;
        assert_eq!(pulses[idx], FOOTER, "section footer");
        idx += 1;
    }
}
