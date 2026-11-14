use super::catalogue::APK_DB;
use super::*;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;
use walpad_hw::{sysinfo, Root};

fn cat_json() -> &'static str {
    r#"{"version":1,"reserve_kb":2097152,"entries":[
    {"id":"browser","name":"Browser","category":"Built in","packages":["firefox-esr"],"size_kb":224000,"exec":["firefox-esr"],"wm_class":"firefox-esr","desktop":"firefox-esr.desktop","roles":["browser"],"system":true},
    {"id":"mousepad","name":"Mousepad","category":"Documents","packages":["mousepad"],"size_kb":700,"exec":["mousepad"],"wm_class":"mousepad","desktop":"org.xfce.mousepad.desktop","icon":"apps"},
    {"id":"evince","name":"Evince","category":"Documents","packages":["evince"],"size_kb":15000,"exec":["evince"],"desktop":"org.gnome.Evince.desktop","roles":["pdf"]},
    {"id":"zathura","name":"Zathura","category":"Documents","packages":["zathura","zathura-pdf-mupdf"],"size_kb":1500,"exec":["zathura"],"desktop":"org.pwmt.zathura.desktop","roles":["pdf"]},
    {"id":"font-x","name":"Font","category":"Fonts","packages":["font-x"],"size_kb":100,"exec":[]},
    {"id":"evil","name":"Evil","category":"X","packages":["--allow-untrusted"],"size_kb":1,"exec":["x"]},
    {"id":"evil2","name":"Evil","category":"X","packages":["/tmp/x.apk"],"size_kb":1,"exec":["x"]},
    {"id":"Bad Id","packages":["x"]},
    {"id":"mousepad","name":"dup","packages":["other"]}
    ]}"#
}

fn root(tag: &str) -> Root {
    let d = std::env::temp_dir().join(format!("walpad-pkg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("etc/walpad")).unwrap();
    std::fs::create_dir_all(d.join("lib/apk/db")).unwrap();
    std::fs::write(d.join("etc/walpad/catalogue.json"), cat_json()).unwrap();
    Root::new(d)
}

fn db(r: &Root, pk: &[&str]) {
    let t: String = pk.iter().map(|p| format!("C:x\nP:{p}\nV:1\n\n")).collect();
    std::fs::write(r.p(APK_DB), t).unwrap();
}

#[test]
fn catalogue_drops_entries_that_could_reach_apk_wrongly() {
    let r = root("load");
    let c = Catalogue::load(&r);
    let ids: Vec<_> = c.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["browser", "mousepad", "evince", "zathura", "font-x"]);
    assert_eq!(
        c.get("mousepad").unwrap().size_kb,
        700,
        "first duplicate wins"
    );
    assert_eq!(c.reserve_kb, RESERVE_KB);
    for bad in [
        "",
        "-x",
        "--allow-untrusted",
        "a b",
        "/tmp/x.apk",
        "x.apk",
        "A",
        "a;b",
        "a/b",
    ] {
        assert!(!valid_package(bad), "{bad:?}");
    }
    assert!(valid_package("libreoffice-writer") && valid_package("gtk+3.0"));
    // the shipped catalogue is entirely valid
    let shipped = Root::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rootfs"));
    let n = std::fs::read_to_string(shipped.p(CATALOGUE))
        .unwrap()
        .matches("\"id\":")
        .count();
    assert_eq!(Catalogue::load(&shipped).entries.len(), n);
    assert!(n >= 25);
}

#[test]
fn installed_state_comes_from_the_apk_database() {
    let r = root("db");
    let c = Catalogue::load(&r);
    assert!(installed_packages(&r).is_empty());
    db(&r, &["mousepad", "zathura", "busybox"]);
    let pk = installed_packages(&r);
    assert!(is_installed(c.get("mousepad").unwrap(), &pk));
    assert!(
        !is_installed(c.get("zathura").unwrap(), &pk),
        "needs both packages"
    );
    assert!(is_installed(c.get("browser").unwrap(), &pk), "built in");
    assert!(!is_installed(c.get("evince").unwrap(), &pk));
}

#[test]
fn free_space_rule() {
    const GIB: u64 = 1024 * 1024;
    // the scenario: needs 3 GiB with 4 GiB free leaves 1 GiB: refused
    let e = check_space(4 * GIB, 3 * GIB, 2 * GIB).unwrap_err();
    assert!(
        e.contains("Not enough free space") && e.contains("3.0 GB") && e.contains("4.0 GB"),
        "{e}"
    );
    assert!(
        check_space(5 * GIB, 3 * GIB, 2 * GIB).is_ok(),
        "exactly the reserve is fine"
    );
    assert!(
        check_space(3 * GIB, 4 * GIB, 2 * GIB).is_err(),
        "no underflow"
    );
    assert!(check_space(10 * GIB, 0, 2 * GIB).is_ok());
}

#[test]
fn app_entry_is_generated_from_the_catalogue() {
    let c = Catalogue::load(&root("json"));
    let j = c.get("mousepad").unwrap().app_json().unwrap();
    assert_eq!(
        j,
        json!({"id":"mousepad","name":"Mousepad","icon":"apps","exec":["mousepad"],"env":{},"single_instance":true,"wm_class":"mousepad"})
    );
    assert!(
        c.get("font-x").unwrap().app_json().is_none(),
        "packages only: no icon"
    );
}

#[test]
fn open_entries_register_with_a_fixed_argv_tail() {
    let mk = |open: serde_json::Value| {
        Entry::parse(&json!({"id":"p","packages":["p"],"exec":["p"],"open":open})).unwrap()
    };
    let j = mk(json!(true)).app_json().unwrap();
    assert_eq!(j["open"], json!(true));
    assert_eq!(j["open_args"], json!(["--", "{path}"]));
    let j = mk(json!(["--file", "{path}"])).app_json().unwrap();
    assert_eq!(j["open_args"], json!(["--file", "{path}"]));
    // no placeholder, two of them or a non-list: the app does not open files
    for bad in [
        json!(["--"]),
        json!(["{path}", "{path}"]),
        json!("yes"),
        json!(false),
    ] {
        let j = mk(bad).app_json().unwrap();
        assert!(j.get("open").is_none());
    }
    let shipped: Value = serde_json::from_str(include_str!(
        "../../../../config/rootfs/etc/walpad/catalogue.json"
    ))
    .unwrap();
    let ids: Vec<&str> = ["mpv", "gpicview"].to_vec();
    for e in shipped["entries"].as_array().unwrap() {
        if ids.contains(&e["id"].as_str().unwrap()) {
            assert!(Entry::parse(e).unwrap().open.is_some(), "{}", e["id"]);
        }
    }
}

#[test]
fn effective_default_falls_back_to_an_installed_candidate() {
    let r = root("eff");
    let c = Catalogue::load(&r);
    let mut d = Defaults::default();
    let none = HashSet::new();
    assert_eq!(effective("pdf", &c, &none, &d), None);
    assert_eq!(
        effective("browser", &c, &none, &d).as_deref(),
        Some("browser")
    );
    let pk: HashSet<String> = ["evince".to_string()].into();
    assert_eq!(effective("pdf", &c, &pk, &d).as_deref(), Some("evince"));
    d.roles.insert("pdf".into(), "zathura".into());
    assert_eq!(
        effective("pdf", &c, &pk, &d).as_deref(),
        Some("evince"),
        "the chosen app is not installed: fall back"
    );
    let both: HashSet<String> = ["evince", "zathura", "zathura-pdf-mupdf"]
        .map(String::from)
        .into();
    assert_eq!(effective("pdf", &c, &both, &d).as_deref(), Some("zathura"));
}

#[test]
fn default_app_writes_defaults_and_mimeapps() {
    let r = root("defaults");
    db(&r, &["evince", "zathura", "zathura-pdf-mupdf"]);
    let mut p = Pkg::new(&r);
    p.set_default("pdf", "zathura").unwrap();
    assert_eq!(Defaults::load(&r).roles["pdf"], "zathura");
    let m = std::fs::read_to_string(r.p(MIMEAPPS)).unwrap();
    assert!(m.starts_with("[Default Applications]\n"));
    assert!(
        m.contains("application/pdf=org.pwmt.zathura.desktop;\n"),
        "{m}"
    );
    assert!(
        m.contains("x-scheme-handler/https=firefox-esr.desktop;"),
        "{m}"
    );
    p.set_default("pdf", "evince").unwrap();
    assert!(std::fs::read_to_string(r.p(MIMEAPPS))
        .unwrap()
        .contains("application/pdf=org.gnome.Evince.desktop;\n"));
    // refusals
    assert!(matches!(
        p.set_default("shell", "evince"),
        Err(PkgError::Unknown(_))
    ));
    assert!(matches!(
        p.set_default("pdf", "nope"),
        Err(PkgError::Unknown(_))
    ));
    assert!(
        matches!(p.set_default("pdf", "mousepad"), Err(PkgError::Refused(_))),
        "no such role"
    );
    db(&r, &["evince"]);
    assert!(
        matches!(p.set_default("pdf", "zathura"), Err(PkgError::Refused(_))),
        "not installed"
    );
}

#[test]
fn catalogue_answer_lists_state_and_heals_a_missing_registration() {
    let r = root("answer");
    db(&r, &["mousepad"]);
    let p = Pkg::new(&r);
    let v = p.catalogue_json();
    let e = v["entries"].as_array().unwrap();
    let get = |id: &str| e.iter().find(|x| x["id"] == id).unwrap().clone();
    assert_eq!(get("mousepad")["installed"], true);
    assert_eq!(get("evince")["installed"], false);
    assert_eq!(get("browser")["system"], true);
    assert_eq!(v["busy"], Value::Null);
    assert_eq!(v["reserve_kb"], RESERVE_KB);
    assert_eq!(v["defaults"]["browser"], "browser");
    assert_eq!(v["defaults"]["pdf"], Value::Null);
    assert!(
        r.p("/var/lib/walpad/apps.d/mousepad.json").is_file(),
        "an installed catalogue app without an apps.d entry is registered again"
    );
    assert!(!r.p("/var/lib/walpad/apps.d/font-x.json").exists());
}

#[test]
fn start_refuses_what_it_must_not_run() {
    let r = root("refuse");
    db(&r, &["mousepad"]);
    let mut p = Pkg::new(&r);
    let now = Instant::now();
    assert!(matches!(
        p.start(Op::Install, "nope", now),
        Err(PkgError::Unknown(_))
    ));
    assert!(
        matches!(
            p.start(Op::Install, "browser", now),
            Err(PkgError::Refused(_))
        ),
        "built in"
    );
    assert!(
        matches!(
            p.start(Op::Remove, "browser", now),
            Err(PkgError::Refused(_))
        ),
        "system app"
    );
    assert!(
        matches!(
            p.start(Op::Install, "mousepad", now),
            Err(PkgError::Refused(_))
        ),
        "already installed"
    );
    assert!(
        matches!(
            p.start(Op::Remove, "evince", now),
            Err(PkgError::Refused(_))
        ),
        "not installed"
    );
    // the 3 GiB with 4 GiB free case, through the reserve hook (the host's free space varies)
    let (_, free) = sysinfo::disk_usage(&r).unwrap();
    p.set_reserve_kb(free / 1024 + 1);
    let e = p.start(Op::Install, "evince", now);
    assert!(
        matches!(&e, Err(PkgError::Refused(m)) if m.contains("Not enough free space")),
        "{e:?}"
    );
    assert!(p.job.is_none(), "nothing was started");
}
