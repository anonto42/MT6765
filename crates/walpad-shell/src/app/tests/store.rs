//! App store screen: Settings > Apps and the Tools tile (change app-catalogue-manager).
use super::*;
use crate::panels;
use crate::store::{StoreTarget, StoreView};

pub const GIB: u64 = 1024 * 1024;

pub fn catalogue_data(free_kb: u64, zathura: bool, busy: Value) -> Value {
    json!({
        "entries": [
            {"id":"browser","name":"Browser","category":"Built in","size_kb":224000,"installed":true,"system":true,"roles":["browser"]},
            {"id":"mousepad","name":"Mousepad","category":"Documents","summary":"Simple text editor","size_kb":700,"installed":true,"system":false,"roles":[]},
            {"id":"evince","name":"Evince","category":"Documents","summary":"PDF viewer","size_kb":15000,"installed":true,"system":false,"roles":["pdf"]},
            {"id":"zathura","name":"Zathura","category":"Documents","summary":"Light PDF viewer","size_kb":1500,"installed":zathura,"system":false,"roles":["pdf"]},
            {"id":"mpv","name":"mpv","category":"Media","summary":"Video player","size_kb":37600,"installed":false,"system":false,"roles":["video"]},
            {"id":"huge","name":"Huge Suite","category":"Office","summary":"Everything","size_kb": 3 * GIB,"installed":false,"system":false,"roles":[]}
        ],
        "free_kb": free_kb, "reserve_kb": 2 * GIB, "busy": busy,
        "defaults": {"browser":"browser","pdf":"evince"}
    })
}

pub fn load_catalogue(app: &mut App, data: Value) {
    app.note_sent(90, &Cmd::PkgCatalogue);
    reply(app, 90, data);
}

fn entry(app: &App, id: &str) -> usize {
    app.model
        .store
        .entries
        .iter()
        .position(|e| e.id == id)
        .unwrap()
}

fn open_settings_apps(app: &mut App) -> Outcome {
    tap(app, Target::Nav(crate::chrome::NAV_SETTINGS));
    tap(app, Target::Page(Page::Apps))
}

#[test]
fn settings_apps_page_lists_installed_apps_and_asks_for_the_catalogue() {
    let mut app = online();
    let out = open_settings_apps(&mut app);
    assert_eq!(out.cmds, vec![Cmd::PkgCatalogue]);
    assert_eq!(Cmd::PkgCatalogue.request(), ("pkg_catalogue", Value::Null));
    assert_eq!(app.model.settings_page, Some(Page::Apps));
    load_catalogue(&mut app, catalogue_data(5 * GIB, false, Value::Null));
    let st = &app.model.store;
    assert!(st.loaded && st.view == StoreView::Installed);
    // built-in apps have no uninstall, others do
    assert!(app
        .content_hits
        .iter()
        .all(|(_, t)| *t != Target::Store(StoreTarget::Remove(0))));
    assert!(app
        .content_hits
        .iter()
        .any(|(_, t)| *t == Target::Store(StoreTarget::Remove(1))));
    // Back from the page leaves Settings > Apps
    tap(&mut app, Target::Back);
    assert_eq!(app.model.settings_page, None);
}

#[test]
fn tools_tile_opens_the_catalogue() {
    let mut app = online();
    tap(&mut app, Target::Nav(crate::chrome::NAV_APPS));
    let tool = panels::TOOLS
        .iter()
        .position(|t| t.action == panels::ToolAction::AppStore)
        .unwrap();
    let out = tap(&mut app, Target::PanelTool(tool));
    assert_eq!(out.cmds, vec![Cmd::PkgCatalogue]);
    assert!(app.model.overlay.is_none());
    assert_eq!(app.active, Tab::Settings);
    assert_eq!(app.model.settings_page, Some(Page::Apps));
    assert_eq!(app.model.store.view, StoreView::Browse);
    load_catalogue(&mut app, catalogue_data(5 * GIB, false, Value::Null));
    // Back goes to the installed list first, then out of Settings > Apps
    tap(&mut app, Target::Back);
    assert_eq!(app.model.store.view, StoreView::Installed);
    assert_eq!(app.model.settings_page, Some(Page::Apps));
    tap(&mut app, Target::Back);
    assert_eq!(app.model.settings_page, None);
}

#[test]
fn install_shows_progress_then_reloads_the_app_list() {
    let mut app = online();
    open_settings_apps(&mut app);
    load_catalogue(&mut app, catalogue_data(5 * GIB, false, Value::Null));
    tap(&mut app, Target::Store(StoreTarget::Browse));
    assert_eq!(app.model.store.view, StoreView::Browse);
    let z = entry(&app, "zathura");
    let out = tap(&mut app, Target::Store(StoreTarget::Install(z)));
    assert_eq!(out.cmds, vec![Cmd::PkgInstall("zathura".into())]);
    assert_eq!(
        out.cmds[0].request(),
        ("pkg_install", json!({"id": "zathura"})),
        "only the catalogue id goes to the core"
    );
    assert_eq!(app.model.store.busy.as_ref().unwrap().id, "zathura");
    // while it runs, a second install is not started
    let m = entry(&app, "mpv");
    assert!(app
        .content_hits
        .iter()
        .any(|(_, t)| *t == Target::Store(StoreTarget::Install(m))));
    assert!(tap(&mut app, Target::Store(StoreTarget::Install(m)))
        .cmds
        .is_empty());
    let o = app.on_core(
        &CoreEvent::Notice(json!({"kind":"pkg_progress","op":"install","id":"zathura","percent":60,"text":"Installing zathura"})),
        3000,
    );
    assert!(o.dirty);
    render(&mut app);
    assert_eq!(app.model.store.busy.as_ref().unwrap().percent, 60);
    let o = app.on_core(
        &CoreEvent::Notice(
            json!({"kind":"pkg_done","op":"install","id":"zathura","ok":true,"error":null}),
        ),
        4000,
    );
    assert!(o.refresh, "apps.d changed: the shell re-reads the app list");
    assert_eq!(o.cmds, vec![Cmd::PkgCatalogue]);
    assert!(app.model.store.busy.is_none() && app.model.store.error.is_none());
    assert!(app
        .model
        .toast
        .as_ref()
        .unwrap()
        .text
        .contains("Zathura installed"));
    load_catalogue(&mut app, catalogue_data(5 * GIB, true, Value::Null));
    let z = entry(&app, "zathura");
    assert!(
        app.content_hits
            .iter()
            .all(|(_, t)| *t != Target::Store(StoreTarget::Install(z))),
        "installed: no Install button"
    );
}

#[test]
fn failed_install_shows_the_error_and_can_be_dismissed() {
    let mut app = online();
    open_settings_apps(&mut app);
    load_catalogue(&mut app, catalogue_data(5 * GIB, false, Value::Null));
    tap(&mut app, Target::Store(StoreTarget::Browse));
    let m = entry(&app, "mpv");
    tap(&mut app, Target::Store(StoreTarget::Install(m)));
    app.on_core(
        &CoreEvent::Notice(json!({"kind":"pkg_done","op":"install","id":"mpv","ok":false,"error":"Could not reach the package repository"})),
        4000,
    );
    render(&mut app);
    assert!(app
        .model
        .store
        .error
        .as_ref()
        .unwrap()
        .contains("Could not reach"));
    assert!(app.model.store.busy.is_none());
    assert_eq!(app.model.toast.as_ref().unwrap().tone, Tone::Danger);
    tap(&mut app, Target::Store(StoreTarget::DismissError));
    assert!(app.model.store.error.is_none());
    // a refusal from the core (busy, no space...) also clears the optimistic busy state
    let out = tap(&mut app, Target::Store(StoreTarget::Install(m)));
    assert_eq!(out.cmds.len(), 1);
    app.note_sent(77, &out.cmds[0]);
    reply_err(&mut app, 77, "refused", "Not enough free space: needs more");
    assert!(app.model.store.busy.is_none());
    assert!(app
        .model
        .store
        .error
        .as_ref()
        .unwrap()
        .contains("Not enough free space"));
}

#[test]
fn insufficient_space_is_explained_and_nothing_is_sent() {
    let mut app = online();
    open_settings_apps(&mut app);
    // 3 GiB needed with 4 GiB free: 1 GiB would remain, 2 GiB must
    load_catalogue(&mut app, catalogue_data(4 * GIB, false, Value::Null));
    tap(&mut app, Target::Store(StoreTarget::Browse));
    let h = entry(&app, "huge");
    let out = tap(&mut app, Target::Store(StoreTarget::Install(h)));
    assert!(out.cmds.is_empty(), "no request, nothing changes");
    let e = app.model.store.error.clone().unwrap();
    assert!(
        e.contains("Not enough free space") && e.contains("3.0 GB") && e.contains("4.0 GB"),
        "{e}"
    );
    assert!(app.model.store.busy.is_none());
}

#[test]
fn uninstall_needs_a_second_tap_and_never_for_built_in_apps() {
    let mut app = online();
    open_settings_apps(&mut app);
    load_catalogue(&mut app, catalogue_data(5 * GIB, false, Value::Null));
    let m = entry(&app, "mousepad");
    let out = tap(&mut app, Target::Store(StoreTarget::Remove(m)));
    assert!(out.cmds.is_empty());
    assert_eq!(app.model.store.armed, Some(m));
    // tapping something else disarms
    tap(&mut app, Target::Store(StoreTarget::Browse));
    assert_eq!(app.model.store.armed, None);
    tap(&mut app, Target::Back);
    tap(&mut app, Target::Store(StoreTarget::Remove(m)));
    let out = tap(&mut app, Target::Store(StoreTarget::Remove(m)));
    assert_eq!(out.cmds, vec![Cmd::PkgRemove("mousepad".into())]);
    assert_eq!(
        out.cmds[0].request(),
        ("pkg_remove", json!({"id": "mousepad"}))
    );
    // a system entry cannot be removed even if the target were sent
    let b = entry(&app, "browser");
    app.model.store.busy = None;
    let mut o = Outcome::default();
    app.on_store_tap(StoreTarget::Remove(b), &mut o);
    app.on_store_tap(StoreTarget::Remove(b), &mut o);
    assert!(o.cmds.is_empty());
}

#[test]
fn default_app_is_chosen_from_the_installed_candidates() {
    let mut app = online();
    open_settings_apps(&mut app);
    load_catalogue(&mut app, catalogue_data(5 * GIB, true, Value::Null));
    let pdf = walpad_proto::DEFAULT_ROLES
        .iter()
        .position(|r| *r == "pdf")
        .unwrap();
    tap(&mut app, Target::Store(StoreTarget::Role(pdf)));
    assert_eq!(app.model.store.view, StoreView::Role(pdf));
    let z = entry(&app, "zathura");
    let e = entry(&app, "evince");
    assert!(
        app.content_hits
            .iter()
            .all(|(_, t)| *t != Target::Store(StoreTarget::PickDefault(e))),
        "the current default is not a button"
    );
    let out = tap(&mut app, Target::Store(StoreTarget::PickDefault(z)));
    assert_eq!(
        out.cmds,
        vec![Cmd::DefaultApp {
            role: "pdf".into(),
            app: "zathura".into()
        }]
    );
    assert_eq!(
        out.cmds[0].request(),
        ("default_app", json!({"role": "pdf", "app": "zathura"}))
    );
    app.note_sent(78, &out.cmds[0]);
    let o = reply(&mut app, 78, json!({"role":"pdf","app":"zathura"}));
    assert_eq!(
        o.cmds,
        vec![Cmd::PkgCatalogue],
        "refresh the effective defaults"
    );
}

#[test]
fn offline_the_store_is_read_only() {
    let mut app = online();
    open_settings_apps(&mut app);
    load_catalogue(&mut app, catalogue_data(5 * GIB, false, Value::Null));
    tap(&mut app, Target::Store(StoreTarget::Browse));
    app.on_core(&CoreEvent::Disconnected, 5000);
    render(&mut app);
    let m = entry(&app, "mpv");
    assert!(tap(&mut app, Target::Store(StoreTarget::Install(m)))
        .cmds
        .is_empty());
}

#[test]
fn app_list_follows_apps_d() {
    let d = std::env::temp_dir().join(format!("walpad-store-apps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("etc/walpad")).unwrap();
    std::fs::create_dir_all(d.join("var/lib/walpad/apps.d")).unwrap();
    std::fs::write(
        d.join("etc/walpad/apps.json"),
        r#"[{"id":"browser","name":"Browser","icon":"globe","exec":["x"]}]"#,
    )
    .unwrap();
    std::fs::write(d.join("var/lib/walpad/apps.d/mousepad.json"), r#"{"id":"mousepad","name":"Mousepad","icon":"apps","exec":["mousepad"],"wm_class":"mousepad"}"#).unwrap();
    let cat = load_catalog(&Root::new(&d));
    assert_eq!(
        cat.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["browser", "mousepad"]
    );
    let mut app = online();
    app.on_core(
        &CoreEvent::Status(Box::new(status(
            120,
            &[("browser", false), ("mousepad", false)],
        ))),
        1,
    );
    assert!(app.set_catalog(cat));
    assert!(app
        .model
        .apps
        .iter()
        .any(|t| t.id == "mousepad" && t.name == "Mousepad"));
}
