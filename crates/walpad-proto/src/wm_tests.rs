//! Parsing and validation of the window commands (change multitasking-windows).
use super::*;
use crate::wm::{LayoutMode, SnapZone};

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
fn instance_and_window_commands_parse() {
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"new_instance","args":{"app":"terminal"}}"#)
            .unwrap()
            .1,
        Command::NewInstance {
            app: "terminal".into()
        }
    );
    assert_eq!(
        code(req(
            r#"{"v":1,"id":1,"cmd":"new_instance","args":{"app":"../x"}}"#
        )),
        ErrorCode::InvalidArgs
    );
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"windows"}"#).unwrap().1,
        Command::Windows
    );
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"focus_window","args":{"window":4194305}}"#)
            .unwrap()
            .1,
        Command::FocusWindow { window: 4194305 }
    );
    assert_eq!(
        code(req(
            r#"{"v":1,"id":1,"cmd":"close_window","args":{"window":0}}"#
        )),
        ErrorCode::InvalidArgs
    );
}

#[test]
fn window_rect_is_validated() {
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"window_rect","args":{"window":9,"x":-5,"y":10,"w":400,"h":300}}"#)
            .unwrap()
            .1,
        Command::WindowRect {
            window: 9,
            x: -5,
            y: 10,
            w: 400,
            h: 300
        }
    );
    for bad in [
        r#"{"window":9,"x":0,"y":0,"w":0,"h":300}"#,
        r#"{"window":9,"x":0,"y":0,"w":400,"h":99999}"#,
        r#"{"window":9,"x":99999,"y":0,"w":400,"h":300}"#,
        r#"{"window":9,"x":0,"y":0,"w":400}"#,
    ] {
        let line = format!(r#"{{"v":1,"id":1,"cmd":"window_rect","args":{bad}}}"#);
        assert_eq!(code(req(&line)), ErrorCode::InvalidArgs, "{bad}");
    }
}

#[test]
fn snap_and_layout_parse() {
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"snap","args":{"window":9,"zone":"left"}}"#)
            .unwrap()
            .1,
        Command::Snap {
            window: 9,
            zone: SnapZone::Left
        }
    );
    assert_eq!(
        code(req(
            r#"{"v":1,"id":1,"cmd":"snap","args":{"window":9,"zone":"middle"}}"#
        )),
        ErrorCode::InvalidArgs
    );
    // no args is a read; mode, ratio and swap are optional
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"layout"}"#).unwrap().1,
        Command::Layout {
            mode: None,
            ratio: None,
            swap: false
        }
    );
    assert_eq!(
        req(r#"{"v":1,"id":1,"cmd":"layout","args":{"mode":"split","ratio":67,"swap":true}}"#)
            .unwrap()
            .1,
        Command::Layout {
            mode: Some(LayoutMode::Split),
            ratio: Some(67),
            swap: true
        }
    );
    for bad in [r#"{"mode":"grid"}"#, r#"{"ratio":40}"#] {
        let line = format!(r#"{{"v":1,"id":1,"cmd":"layout","args":{bad}}}"#);
        assert_eq!(code(req(&line)), ErrorCode::InvalidArgs, "{bad}");
    }
}
