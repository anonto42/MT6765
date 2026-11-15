use super::*;

fn evt(code: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![EVENT, code, payload.len() as u8];
    v.extend_from_slice(payload);
    v
}
fn acl(handle: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![ACL];
    v.extend_from_slice(&handle.to_le_bytes());
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}
fn sco(handle: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![SCO];
    v.extend_from_slice(&handle.to_le_bytes());
    v.push(payload.len() as u8);
    v.extend_from_slice(payload);
    v
}
fn cmd(opcode: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![CMD];
    v.extend_from_slice(&opcode.to_le_bytes());
    v.push(payload.len() as u8);
    v.extend_from_slice(payload);
    v
}
fn drain(d: &mut Deframer) -> Vec<Next> {
    std::iter::from_fn(|| d.next_packet()).collect()
}
fn packets(d: &mut Deframer) -> Vec<Vec<u8>> {
    drain(d)
        .into_iter()
        .map(|n| match n {
            Next::Packet(p) => p,
            Next::Dropped(e) => panic!("unexpected drop: {e}"),
        })
        .collect()
}

#[test]
fn lengths_of_all_four_types() {
    let all = &[CMD, ACL, SCO, EVENT];
    assert_eq!(frame_len(&cmd(0x0c03, &[]), all, 4096), Ok(Some(4)));
    assert_eq!(frame_len(&acl(1, &[0; 300]), all, 4096), Ok(Some(305)));
    assert_eq!(frame_len(&sco(1, &[0; 60]), all, 4096), Ok(Some(64)));
    assert_eq!(
        frame_len(&evt(0x0e, &[1, 3, 0x0c, 0]), all, 4096),
        Ok(Some(7))
    );
    assert_eq!(frame_len(&[VENDOR, 0, 0, 0], &[VENDOR], 4096), Ok(Some(4)));
}

#[test]
fn incomplete_headers_wait() {
    let all = &[CMD, ACL, SCO, EVENT];
    assert_eq!(frame_len(&[], all, 4096), Ok(None));
    assert_eq!(frame_len(&[CMD, 3, 0x0c], all, 4096), Ok(None));
    assert_eq!(frame_len(&[ACL, 1, 0, 5], all, 4096), Ok(None));
    assert_eq!(frame_len(&[SCO, 1, 0], all, 4096), Ok(None));
    assert_eq!(frame_len(&[EVENT, 0x0e], all, 4096), Ok(None));
}

#[test]
fn merged_packets_in_one_read() {
    let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
    let (a, b, c) = (
        evt(0x0e, &[1, 3, 0x0c, 0]),
        acl(0x2001, b"hello"),
        sco(0x0006, &[7; 48]),
    );
    let mut stream = a.clone();
    stream.extend(&b);
    stream.extend(&c);
    stream.extend(&a);
    d.push(&stream);
    assert_eq!(packets(&mut d), vec![a.clone(), b, c, a]);
    assert_eq!(d.pending(), 0);
}

#[test]
fn packet_split_byte_by_byte() {
    let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
    let p = acl(0x2001, &[0xab; 1021]);
    for (i, byte) in p.iter().enumerate() {
        d.push(&[*byte]);
        let got = packets(&mut d);
        if i + 1 < p.len() {
            assert!(got.is_empty(), "emitted early at byte {i}");
        } else {
            assert_eq!(got, vec![p.clone()]);
        }
    }
}

#[test]
fn split_across_header_and_merged_tail() {
    let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
    let (a, b) = (evt(0x3e, &[0x02; 40]), acl(0x2002, &[1, 2, 3]));
    let mut stream = a.clone();
    stream.extend(&b);
    d.push(&stream[..2]); // type + event code only
    assert!(packets(&mut d).is_empty());
    d.push(&stream[2..a.len() + 3]); // rest of a + start of b's header
    assert_eq!(packets(&mut d), vec![a.clone()]);
    d.push(&stream[a.len() + 3..]);
    assert_eq!(packets(&mut d), vec![b]);
}

#[test]
fn zero_length_payloads() {
    let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
    let (a, b, c) = (evt(0x10, &[]), acl(1, &[]), sco(1, &[]));
    d.push(&[a.clone(), b.clone(), c.clone()].concat());
    assert_eq!(packets(&mut d), vec![a, b, c]);
}

#[test]
fn oversize_acl_is_dropped_and_resyncs() {
    let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
    // ACL header announcing 2000 bytes (> 1028 total): not forwardable to vhci
    let bad = [ACL, 0x01, 0x20, 0xd0, 0x07];
    let good = evt(0x0e, &[1, 3, 0x0c, 0]);
    d.push(&bad);
    d.push(&good);
    let out = drain(&mut d);
    assert_eq!(
        out[0],
        Next::Dropped(FrameError::Oversize {
            ptype: ACL,
            total: 2005
        })
    );
    assert_eq!(out.last(), Some(&Next::Packet(good)));
    assert!(d.dropped_bytes >= 1);
}

#[test]
fn exactly_max_is_accepted() {
    let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
    let p = acl(1, &[0; VHCI_MAX_FRAME - 5]);
    d.push(&p);
    assert_eq!(packets(&mut d), vec![p]);
}

#[test]
fn unknown_type_resyncs() {
    let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
    let good = evt(0x0f, &[0, 1, 5, 4]);
    d.push(&[0x00, 0x99]);
    d.push(&good);
    let out = drain(&mut d);
    assert_eq!(out[0], Next::Dropped(FrameError::UnknownType(0x00)));
    assert_eq!(out[1], Next::Dropped(FrameError::UnknownType(0x99)));
    assert_eq!(out[2], Next::Packet(good));
    assert_eq!(d.dropped_bytes, 2);
}

#[test]
fn command_is_not_a_controller_packet() {
    let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
    d.push(&[CMD]);
    assert_eq!(
        d.next_packet(),
        Some(Next::Dropped(FrameError::UnknownType(CMD)))
    );
}

#[test]
fn single_packet_check_for_vhci_reads() {
    assert!(check_single(&cmd(0x0c03, &[]), FROM_HOST, STPBT_MAX_WRITE).is_ok());
    assert!(check_single(&acl(1, &[0; 1021]), FROM_HOST, STPBT_MAX_WRITE).is_ok());
    assert!(check_single(&[VENDOR, 0, 0, 0], FROM_HOST, STPBT_MAX_WRITE).is_ok());
    assert!(check_single(&cmd(0x0c03, &[1])[..4], FROM_HOST, STPBT_MAX_WRITE).is_err());
    assert!(check_single(
        &[cmd(0x0c03, &[]), cmd(0x0c03, &[])].concat(),
        FROM_HOST,
        STPBT_MAX_WRITE
    )
    .is_err());
    assert!(check_single(&evt(0x0e, &[]), FROM_HOST, STPBT_MAX_WRITE).is_err());
    assert!(check_single(&acl(1, &[0; 3000]), FROM_HOST, STPBT_MAX_WRITE).is_err());
}

#[test]
fn sync_train_failure_is_turned_into_success() {
    let failed_cc = evt(0x0e, &[1, 0x77, 0x0c, 0x01]);
    let fixed = fixup_sync_train(&failed_cc).unwrap();
    assert_eq!(
        frame_len(&fixed, FROM_CONTROLLER, VHCI_MAX_FRAME),
        Ok(Some(fixed.len()))
    );
    assert_eq!(
        command_complete_status(&fixed, READ_SYNC_TRAIN_PARAMS),
        Some(0)
    );
    assert_eq!(fixed.len(), 3 + 11);
    let failed_cs = evt(0x0f, &[0x11, 2, 0x77, 0x0c]);
    assert_eq!(fixup_sync_train(&failed_cs).unwrap()[3], 2); // keeps the controller's ncmd
                                                             // a successful reply and other commands pass through untouched
    assert_eq!(
        fixup_sync_train(&evt(0x0e, &[1, 0x77, 0x0c, 0, 0x80, 0, 0, 0xee, 2, 0, 0])),
        None
    );
    assert_eq!(fixup_sync_train(&evt(0x0e, &[1, 0x03, 0x0c, 1])), None);
    assert_eq!(fixup_sync_train(&evt(0x0f, &[0x01, 1, 0x03, 0x0c])), None);
}

#[test]
fn bdaddr_and_command_complete() {
    assert_eq!(
        parse_bdaddr("00:11:22:33:44:5A"),
        Some([0x5a, 0x44, 0x33, 0x22, 0x11, 0x00])
    );
    assert_eq!(parse_bdaddr("00:11:22:33:44"), None);
    assert_eq!(parse_bdaddr("00:11:22:33:44:zz"), None);
    let c = set_bdaddr_cmd([1, 2, 3, 4, 5, 6]);
    assert_eq!(c, vec![CMD, 0x1a, 0xfc, 6, 1, 2, 3, 4, 5, 6]);
    assert_eq!(
        command_complete_status(&evt(0x0e, &[1, 0x1a, 0xfc, 0]), MTK_SET_BDADDR),
        Some(0)
    );
    assert_eq!(
        command_complete_status(&evt(0x0e, &[1, 0x03, 0x0c, 0]), MTK_SET_BDADDR),
        None
    );
    assert_eq!(
        command_complete_status(&evt(0x0f, &[0, 1, 0x1a, 0xfc]), MTK_SET_BDADDR),
        None
    );
}
