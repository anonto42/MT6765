use super::*;
use serde_json::json;

fn req(s: &str) -> Result<(u64, Command), Response> {
    parse_request(s.as_bytes())
}
fn code(r: Result<(u64, Command), Response>) -> ErrorCode {
    r.err()
        .and_then(|e| e.error)
        .map(|e| e.code)
        .unwrap_or(ErrorCode::Internal)
}

#[test]
fn parses_basic_commands() {
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"get_status"}"#).unwrap(),
        (1, Command::GetStatus)
    );
    let (_, c) = req(r#"{"v":1,"id":7,"cmd":"set_brightness","args":{"value":180}}"#).unwrap();
    assert_eq!(
        c,
        Command::SetBrightness(BrightnessArgs {
            value: Some(180),
            delta: None
        })
    );
    let (_, c) = req(r#"{"v":1,"id":2,"cmd":"rotate","args":{"mode":"90"}}"#).unwrap();
    assert_eq!(
        c,
        Command::Rotate {
            mode: RotateMode::R90
        }
    );
}

#[test]
fn rejects_invalid() {
    assert_eq!(
        code(req(
            r#"{"v":1,"id":1,"cmd":"set_brightness","args":{"value":300}}"#
        )),
        ErrorCode::InvalidArgs
    );
    assert_eq!(
        code(req(r#"{"v":1,"id":1,"cmd":"set_brightness"}"#)),
        ErrorCode::InvalidArgs
    );
    assert_eq!(
        code(req(r#"{"v":1,"id":1,"cmd":"rm_rf"}"#)),
        ErrorCode::UnknownCommand
    );
    assert_eq!(
        code(req(r#"{"v":2,"id":1,"cmd":"get_status"}"#)),
        ErrorCode::Version
    );
    assert_eq!(code(req("not json")), ErrorCode::InvalidArgs);
    assert_eq!(
        code(req(
            r#"{"v":1,"id":1,"cmd":"launch","args":{"app":"x; reboot"}}"#
        )),
        ErrorCode::InvalidArgs
    );
    assert_eq!(
        code(req(
            r#"{"v":1,"id":1,"cmd":"set_setting","args":{"key":"/etc/passwd","value":1}}"#
        )),
        ErrorCode::InvalidArgs
    );
    assert_eq!(
        code(req(
            r#"{"v":1,"id":1,"cmd":"set_setting","args":{"key":"dim_after_s","value":-1}}"#
        )),
        ErrorCode::InvalidArgs
    );
    assert_eq!(
        code(req(
            r#"{"v":1,"id":1,"cmd":"wifi_connect","args":{"ssid":"zz"}}"#
        )),
        ErrorCode::InvalidArgs
    );
}

#[test]
fn power_defaults_to_unconfirmed() {
    let (_, c) = req(r#"{"v":1,"id":1,"cmd":"power","args":{"action":"poweroff"}}"#).unwrap();
    assert_eq!(
        c,
        Command::Power {
            action: PowerAction::Poweroff,
            confirmed: false
        }
    );
}

#[test]
fn commands_without_arguments_ignore_a_stray_args_object() {
    for cmd in [
        "get_status",
        "wifi_scan",
        "wifi_list_saved",
        "wifi_disconnect",
        "home",
        "back",
        "restart_shell",
        "screenshot",
    ] {
        for args in [
            r#""args":{}"#,
            r#""args":{"client":"x","proto":1}"#,
            r#""args":null"#,
        ] {
            let line = format!(r#"{{"v":1,"id":1,"cmd":"{cmd}",{args}}}"#);
            assert!(req(&line).is_ok(), "{cmd} with {args}");
        }
    }
    // commands that DO take arguments still validate them
    assert_eq!(
        code(req(r#"{"v":1,"id":1,"cmd":"screen","args":{}}"#)),
        ErrorCode::InvalidArgs
    );
}

#[test]
fn close_app_and_pinned_apps_are_validated() {
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"close_app","args":{"app":"browser"}}"#)
            .unwrap()
            .1,
        Command::CloseApp {
            app: "browser".into()
        }
    );
    assert_eq!(
        code(req(
            r#"{"v":1,"id":1,"cmd":"close_app","args":{"app":"../etc"}}"#
        )),
        ErrorCode::InvalidArgs
    );
    let ok = r#"{"v":1,"id":1,"cmd":"set_setting","args":{"key":"pinned_apps","value":["browser","terminal"]}}"#;
    assert!(req(ok).is_ok());
    for bad in [
        r#"["Browser"]"#,
        r#"["a b"]"#,
        r#"[1]"#,
        r#""browser""#,
        r#"["a","b","c","d","e","f","g","h","i"]"#,
    ] {
        let line = format!(
            r#"{{"v":1,"id":1,"cmd":"set_setting","args":{{"key":"pinned_apps","value":{bad}}}}}"#
        );
        assert_eq!(code(req(&line)), ErrorCode::InvalidArgs, "{bad}");
    }
}

#[test]
fn unknown_fields_ignored() {
    assert!(req(r#"{"v":1,"id":1,"cmd":"get_status","extra":5}"#).is_ok());
}

#[test]
fn secret_is_redacted_everywhere() {
    let (_, c) = req(r#"{"v":1,"id":1,"cmd":"wifi_connect","args":{"ssid":"486f6d65","secret":"hunter2hunter2"}}"#).unwrap();
    let dbg = format!("{c:?}");
    assert!(!dbg.contains("hunter2"), "{dbg}");
    assert!(dbg.contains("***"));
    assert_eq!(format!("{}", Secret::new("x")), "***");
}

#[test]
fn wifi_enable_and_hotspot_commands() {
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"wifi_enable","args":{"on":false}}"#)
            .unwrap()
            .1,
        Command::WifiEnable { on: false }
    );
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"hotspot","args":{"on":true}}"#)
            .unwrap()
            .1,
        Command::Hotspot { on: true }
    );
    for bad in [
        r#"{"v":1,"id":1,"cmd":"wifi_enable"}"#,
        r#"{"v":1,"id":1,"cmd":"wifi_enable","args":{"on":"yes"}}"#,
        r#"{"v":1,"id":1,"cmd":"hotspot","args":{}}"#,
    ] {
        assert_eq!(code(req(bad)), ErrorCode::InvalidArgs, "{bad}");
    }
}

#[test]
fn hotspot_config_is_validated_and_redacted() {
    let (_, c) = req(
        r#"{"v":1,"id":1,"cmd":"hotspot_config","args":{"ssid":"WalPad Café","secret":"hunter2hunter2"}}"#,
    )
    .unwrap();
    assert!(!format!("{c:?}").contains("hunter2"));
    assert!(req(r#"{"v":1,"id":1,"cmd":"hotspot_config","args":{"ssid":"Tab"}}"#).is_ok());
    assert!(req(r#"{"v":1,"id":1,"cmd":"hotspot_config","args":{"secret":"12345678"}}"#).is_ok());
    let long = "x".repeat(63);
    assert!(req(&format!(
        r#"{{"v":1,"id":1,"cmd":"hotspot_config","args":{{"secret":"{long}"}}}}"#
    ))
    .is_ok());
    for bad in [
        r#"{"v":1,"id":1,"cmd":"hotspot_config","args":{}}"#,
        r#"{"v":1,"id":1,"cmd":"hotspot_config","args":{"ssid":""}}"#,
        r#"{"v":1,"id":1,"cmd":"hotspot_config","args":{"ssid":"0123456789012345678901234567890123"}}"#,
        r#"{"v":1,"id":1,"cmd":"hotspot_config","args":{"ssid":"a\nb"}}"#,
        r#"{"v":1,"id":1,"cmd":"hotspot_config","args":{"secret":"short"}}"#,
        r#"{"v":1,"id":1,"cmd":"hotspot_config","args":{"secret":"line\nbreak!"}}"#,
        r#"{"v":1,"id":1,"cmd":"hotspot_config","args":{"secret":"ünicode-pass"}}"#,
    ] {
        let r = req(bad);
        assert_eq!(code(r.clone()), ErrorCode::InvalidArgs, "{bad}");
        // the refusal never echoes the secret
        let msg = r
            .err()
            .and_then(|e| e.error)
            .map(|e| e.msg)
            .unwrap_or_default();
        assert!(!msg.contains("short") && !msg.contains("break"), "{msg}");
    }
    let long = "x".repeat(64);
    assert_eq!(
        code(req(&format!(
            r#"{{"v":1,"id":1,"cmd":"hotspot_config","args":{{"secret":"{long}"}}}}"#
        ))),
        ErrorCode::InvalidArgs
    );
}

#[test]
fn status_without_new_fields_still_parses() {
    let mut v = serde_json::to_value(sample_status()).unwrap();
    v.as_object_mut().unwrap().remove("hotspot");
    v["wifi"].as_object_mut().unwrap().remove("enabled");
    let s: Status = serde_json::from_value(v).unwrap();
    assert!(s.wifi.enabled);
    assert_eq!(s.hotspot.state, HotspotState::Off);
    let v = serde_json::to_value(sample_status()).unwrap();
    assert_eq!(v["hotspot"]["state"], "off");
    assert_eq!(v["wifi"]["enabled"], true);
}

#[test]
fn wire_roundtrip_of_secret() {
    let c = Command::WifiConnect {
        ssid: Some("486f6d65".into()),
        secret: Some(Secret::new("hunter2hunter2")),
        saved_id: None,
    };
    let line = encode_line(&json!({"v":1,"id":3,"cmd":"wifi_connect","args":c_args(&c)}));
    assert!(parse_request(&line[..line.len() - 1]).is_ok());
}
fn c_args(c: &Command) -> serde_json::Value {
    serde_json::to_value(c).unwrap()["args"].clone()
}

#[test]
fn decoder_splits_and_limits() {
    let mut d = LineDecoder::new();
    assert!(d.feed(b"{\"a\":").unwrap().is_empty());
    assert_eq!(
        d.feed(b"1}\n{\"b\":2}\n").unwrap(),
        vec![b"{\"a\":1}".to_vec(), b"{\"b\":2}".to_vec()]
    );
    let big = vec![b'x'; MAX_LINE + 1];
    assert_eq!(d.feed(&big), Err(LineError::TooLong));
    let mut d = LineDecoder::new();
    let ok = vec![b'x'; MAX_LINE];
    assert!(d.feed(&ok).is_ok());
}

#[test]
fn response_shapes() {
    let r = Response::ok(7, json!({"brightness":180}));
    assert_eq!(
        serde_json::to_value(&r).unwrap(),
        json!({"v":1,"id":7,"ok":true,"data":{"brightness":180}})
    );
    let e = Response::err(8, ErrorCode::InvalidArgs, "value must be 0-255");
    assert_eq!(
        serde_json::to_value(&e).unwrap(),
        json!({"v":1,"id":8,"ok":false,"error":{"code":"invalid_args","msg":"value must be 0-255"}})
    );
}

fn sample_status() -> Status {
    Status {
        battery: Battery {
            percent: Some(62),
            source: BatterySource::VoltageEstimate,
            state: Some("Discharging".into()),
            mv: Some(3920),
            temp_c: Some(34.0),
            low: false,
        },
        screen: ScreenState::On,
        brightness: 180,
        orientation: 0,
        rotation_locked: false,
        lid_closed: false,
        wifi: WifiStatus {
            radio: WifiRadio::Up,
            state: Some("COMPLETED".into()),
            ssid: Some("Home".into()),
            ip: Some("192.168.0.100".into()),
            signal_dbm: Some(-58),
            enabled: true,
        },
        hotspot: Default::default(),
        mobile: Default::default(),
        bluetooth: Default::default(),
        audio: Default::default(),
        lock: Default::default(),
        apps: vec![AppState {
            app: "browser".into(),
            running: true,
        }],
        hardware: Hardware {
            touch: "ok".into(),
            gsensor: "ok".into(),
            audio: "unverified".into(),
            bluetooth: "unavailable".into(),
            sd_card: "unavailable".into(),
        },
    }
}

#[test]
fn unknown_battery_is_null() {
    let mut s = sample_status();
    s.battery = Battery {
        percent: None,
        source: BatterySource::Unknown,
        state: None,
        mv: None,
        temp_c: None,
        low: false,
    };
    let v = serde_json::to_value(&s).unwrap();
    assert!(v["battery"]["percent"].is_null());
    assert_eq!(v["battery"]["source"], "unknown");
}

#[test]
fn legacy_status_has_old_field_set() {
    let l = LegacyStatus::from_status(&sample_status(), 100, 12.5, "COMPLETED 192.168.0.100");
    let v = serde_json::to_value(&l).unwrap();
    let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "battery",
            "brightness",
            "lid_closed",
            "low_battery",
            "orientation",
            "rotation_locked",
            "screen",
            "time",
            "uptime_s",
            "wifi"
        ]
    );
    let mut bk: Vec<_> = v["battery"].as_object().unwrap().keys().cloned().collect();
    bk.sort();
    assert_eq!(bk, ["estimated", "mv", "percent", "status", "temp_dC"]);
    assert_eq!(v["battery"]["temp_dC"], "340");
    assert_eq!(v["wifi"], "COMPLETED 192.168.0.100");
}

#[test]
fn mobile_commands_are_validated_and_the_pin_redacted() {
    // owner request 2026-11-10
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"mobile_radio","args":{"on":false}}"#)
            .unwrap()
            .1,
        Command::MobileRadio { on: false }
    );
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"mobile_data","args":{"on":true}}"#)
            .unwrap()
            .1,
        Command::MobileData { on: true }
    );
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"mobile_apn","args":{"apn":"gpinternet"}}"#)
            .unwrap()
            .1,
        Command::MobileApn {
            apn: "gpinternet".into()
        }
    );
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"mobile_info","args":{"x":1}}"#)
            .unwrap()
            .1,
        Command::MobileInfo
    );
    let (_, c) = req(r#"{"v":1,"id":1,"cmd":"sim_pin","args":{"pin":"4711"}}"#).unwrap();
    assert!(!format!("{c:?}").contains("4711"), "the PIN is redacted");
    assert!(req(r#"{"v":1,"id":1,"cmd":"mobile_apn","args":{"apn":"a.b-c_D9"}}"#).is_ok());
    let long = "a".repeat(62);
    assert!(req(&format!(
        r#"{{"v":1,"id":1,"cmd":"mobile_apn","args":{{"apn":"{long}"}}}}"#
    ))
    .is_ok());
    let too_long = format!(
        r#"{{"v":1,"id":1,"cmd":"mobile_apn","args":{{"apn":"{}"}}}}"#,
        "a".repeat(63)
    );
    for bad in [
        r#"{"v":1,"id":1,"cmd":"mobile_radio","args":{}}"#,
        r#"{"v":1,"id":1,"cmd":"mobile_data","args":{"on":"on"}}"#,
        r#"{"v":1,"id":1,"cmd":"mobile_apn","args":{"apn":""}}"#,
        r#"{"v":1,"id":1,"cmd":"mobile_apn","args":{"apn":"inter net"}}"#,
        r#"{"v":1,"id":1,"cmd":"mobile_apn","args":{"apn":"x\";AT+CFUN=0"}}"#,
        too_long.as_str(),
        r#"{"v":1,"id":1,"cmd":"sim_pin","args":{"pin":"123"}}"#,
        r#"{"v":1,"id":1,"cmd":"sim_pin","args":{"pin":"123456789"}}"#,
        r#"{"v":1,"id":1,"cmd":"sim_pin","args":{"pin":"12a4"}}"#,
        r#"{"v":1,"id":1,"cmd":"sim_pin","args":{"pin":"1234\""}}"#,
        r#"{"v":1,"id":1,"cmd":"sim_pin","args":{}}"#,
    ] {
        let r = req(bad);
        assert_eq!(code(r.clone()), ErrorCode::InvalidArgs, "{bad}");
        let msg = r
            .err()
            .and_then(|e| e.error)
            .map(|e| e.msg)
            .unwrap_or_default();
        assert!(!msg.contains("12a4") && !msg.contains("123456789"), "{msg}");
    }
    assert!(valid_sim_pin("00000000") && !valid_sim_pin("０１２３"));
    assert!(valid_apn("internet") && !valid_apn("ünet"));
}

#[test]
fn mobile_status_defaults_and_names() {
    let mut v = serde_json::to_value(sample_status()).unwrap();
    assert_eq!(v["mobile"]["modem"], "absent");
    assert_eq!(v["mobile"]["sim"], "unknown");
    assert_eq!(v["mobile"]["data"], "off");
    v.as_object_mut().unwrap().remove("mobile");
    let s: Status = serde_json::from_value(v).unwrap();
    assert_eq!(s.mobile, MobileStatus::default());
    let m = MobileStatus {
        modem: ModemState::Ready,
        sim: SimState::PinRequired,
        registration: MobileReg::Roaming,
        data: MobileData::Connecting,
        ..Default::default()
    };
    let v = serde_json::to_value(&m).unwrap();
    assert_eq!(
        (&v["modem"], &v["sim"], &v["registration"], &v["data"]),
        (
            &json!("ready"),
            &json!("pin_required"),
            &json!("roaming"),
            &json!("connecting")
        )
    );
}

#[test]
fn bluetooth_commands_are_validated() {
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"bluetooth_enable","args":{"on":true}}"#)
            .unwrap()
            .1,
        Command::BluetoothEnable { on: true }
    );
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"bluetooth_scan","args":{"on":false}}"#)
            .unwrap()
            .1,
        Command::BluetoothScan { on: false }
    );
    for c in [
        "bluetooth_pair",
        "bluetooth_connect",
        "bluetooth_disconnect",
        "bluetooth_forget",
    ] {
        let ok = json!({"v":1,"id":1,"cmd":c,"args":{"address":"aa:BB:0c:DD:ee:F1"}}).to_string();
        assert!(req(&ok).is_ok(), "{c}");
        for bad in [
            "",
            "AA:BB:CC:DD:EE",
            "AA:BB:CC:DD:EE:FF:00",
            "AA-BB-CC-DD-EE-FF",
            "AA:BB:CC:DD:EE:GG",
            "AA:BB:CC:DD:EE:F;",
            "AA:BB:CC:DD:EE:FF\nquit",
            "-h",
            "AA:BB:CC:DD:EE:F\u{e9}",
        ] {
            let l = json!({"v":1,"id":1,"cmd":c,"args":{"address":bad}}).to_string();
            assert_eq!(code(req(&l)), ErrorCode::InvalidArgs, "{c} {bad:?}");
        }
        let missing = json!({"v":1,"id":1,"cmd":c,"args":{}}).to_string();
        assert_eq!(code(req(&missing)), ErrorCode::InvalidArgs);
    }
    assert_eq!(
        req(
            r#"{"v":1,"id":1,"cmd":"bluetooth_confirm","args":{"address":"AA:BB:CC:DD:EE:FF","accept":true}}"#
        )
        .unwrap()
        .1,
        Command::BluetoothConfirm {
            address: "AA:BB:CC:DD:EE:FF".into(),
            accept: true
        }
    );
    assert_eq!(
        code(req(
            r#"{"v":1,"id":1,"cmd":"bluetooth_confirm","args":{"address":"AA:BB:CC:DD:EE:FF"}}"#
        )),
        ErrorCode::InvalidArgs
    );
}

#[test]
fn old_status_without_bluetooth_parses() {
    let mut v = serde_json::to_value(sample_status()).unwrap();
    v.as_object_mut().unwrap().remove("bluetooth");
    let s: Status = serde_json::from_value(v).unwrap();
    assert_eq!(s.bluetooth.controller, BluetoothController::Absent);
    assert!(s.bluetooth.enabled);
}

#[test]
fn volume_commands_are_validated() {
    let (_, c) = req(r#"{"v":1,"id":1,"cmd":"volume_set","args":{"percent":70}}"#).unwrap();
    assert_eq!(c, Command::VolumeSet { percent: 70 });
    assert!(req(r#"{"v":1,"id":1,"cmd":"volume_set","args":{"percent":0}}"#).is_ok());
    assert!(req(r#"{"v":1,"id":1,"cmd":"volume_set","args":{"percent":100}}"#).is_ok());
    let (_, c) = req(r#"{"v":1,"id":2,"cmd":"volume_delta","args":{"delta":-5}}"#).unwrap();
    assert_eq!(c, Command::VolumeDelta { delta: -5 });
    let (_, c) = req(r#"{"v":1,"id":3,"cmd":"mute","args":{"on":true}}"#).unwrap();
    assert_eq!(c, Command::Mute { on: true });
    for bad in [
        r#"{"v":1,"id":1,"cmd":"volume_set","args":{"percent":101}}"#,
        r#"{"v":1,"id":1,"cmd":"volume_set","args":{"percent":-1}}"#,
        r#"{"v":1,"id":1,"cmd":"volume_set","args":{"percent":"loud"}}"#,
        r#"{"v":1,"id":1,"cmd":"volume_set"}"#,
        r#"{"v":1,"id":1,"cmd":"volume_delta","args":{"delta":101}}"#,
        r#"{"v":1,"id":1,"cmd":"volume_delta","args":{"delta":-101}}"#,
        r#"{"v":1,"id":1,"cmd":"mute","args":{}}"#,
        r#"{"v":1,"id":1,"cmd":"mute","args":{"on":1}}"#,
    ] {
        assert_eq!(code(req(bad)), ErrorCode::InvalidArgs, "{bad}");
    }
}

#[test]
fn status_audio_defaults_and_wire_names() {
    let mut v = serde_json::to_value(sample_status()).unwrap();
    assert_eq!(
        v["audio"],
        json!({"volume": 60, "muted": false, "available": false})
    );
    v.as_object_mut().unwrap().remove("audio");
    let s: Status = serde_json::from_value(v).unwrap();
    assert_eq!(s.audio, AudioStatus::default());
}

#[test]
fn lock_commands_are_validated_and_secrets_redacted() {
    // owner request 2026-11-10 (lock-screen)
    let (_, c) =
        req(r#"{"v":1,"id":1,"cmd":"lock_set","args":{"kind":"pin","secret":"4321"}}"#).unwrap();
    assert!(!format!("{c:?}").contains("4321"), "the PIN is redacted");
    let (_, c) = req(
        r#"{"v":1,"id":1,"cmd":"lock_set","args":{"kind":"password","secret":"hunter22x","current":"4321"}}"#,
    )
    .unwrap();
    assert!(!format!("{c:?}").contains("hunter22x") && !format!("{c:?}").contains("4321"));
    assert!(req(r#"{"v":1,"id":1,"cmd":"unlock","args":{"secret":"4321"}}"#).is_ok());
    assert!(req(r#"{"v":1,"id":1,"cmd":"lock_remove","args":{"current":"4321"}}"#).is_ok());
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"lock_now","args":{"x":1}}"#)
            .unwrap()
            .1,
        Command::LockNow
    );
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"lock_config","args":{"auto_lock_s":300}}"#)
            .unwrap()
            .1,
        Command::LockConfig { auto_lock_s: 300 }
    );
    for bad in [
        r#"{"v":1,"id":1,"cmd":"lock_set","args":{"kind":"pin","secret":"123"}}"#,
        r#"{"v":1,"id":1,"cmd":"lock_set","args":{"kind":"pin","secret":"1234567890123"}}"#,
        r#"{"v":1,"id":1,"cmd":"lock_set","args":{"kind":"pin","secret":"12a4"}}"#,
        r#"{"v":1,"id":1,"cmd":"lock_set","args":{"kind":"password","secret":"abc"}}"#,
        r#"{"v":1,"id":1,"cmd":"lock_set","args":{"kind":"pattern","secret":"012"}}"#,
        r#"{"v":1,"id":1,"cmd":"lock_set","args":{"kind":"pattern","secret":"0129"}}"#,
        r#"{"v":1,"id":1,"cmd":"lock_set","args":{"kind":"pattern","secret":"01210"}}"#,
        r#"{"v":1,"id":1,"cmd":"lock_set","args":{"kind":"tattoo","secret":"1234"}}"#,
        r#"{"v":1,"id":1,"cmd":"unlock","args":{"secret":""}}"#,
        r#"{"v":1,"id":1,"cmd":"unlock","args":{}}"#,
        r#"{"v":1,"id":1,"cmd":"lock_config","args":{"auto_lock_s":3601}}"#,
    ] {
        assert!(req(bad).is_err(), "{bad}");
    }
    assert!(valid_lock_secret(LockKind::Pattern, "01258"));
    assert!(valid_lock_secret(LockKind::Pin, "123456789012"));
}

#[test]
fn lock_status_defaults_and_never_carries_a_secret() {
    let l = LockStatus::default();
    assert!(!l.configured && !l.locked && l.kind.is_none() && l.retry_in_s.is_none());
    let v = serde_json::to_value(&l).unwrap();
    let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        ["auto_lock_s", "configured", "kind", "locked", "retry_in_s"]
    );
    let back: LockStatus = serde_json::from_value(serde_json::json!({})).unwrap();
    assert_eq!(back, l);
}
