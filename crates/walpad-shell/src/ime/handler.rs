use super::{IcKey, ImEvent};
use walpad_x::{TimedStream, XConn};
use x11rb::rust_connection::RustConnection;
use xim::x11rb::{HasConnection, X11rbServer};
use xim::{InputStyle, Server, ServerError, ServerHandler, UserInputContext};

/// The shell's timed X connection as the xim crate's transport.
pub struct ImConn(pub(super) XConn);

impl HasConnection for ImConn {
    type Connection = RustConnection<TimedStream>;
    fn conn(&self) -> &Self::Connection {
        &self.0.conn
    }
}

pub(super) type XServer = X11rbServer<ImConn>;

/// Locales the server accepts (Xlib only opens an input method that lists the client's locale,
/// language or full locale name). The server does no text conversion, so any locale works.
pub const LOCALES: &str =
    "C,POSIX,C.UTF-8,C.utf8,en,en_US,en_US.UTF-8,en_US.utf8,en_GB,en_GB.UTF-8,\
en_IN,en_IN.UTF-8,bn,bn_BD,bn_BD.UTF-8,bn_IN,bn_IN.UTF-8,ar,de,de_DE,de_DE.UTF-8,es,es_ES,\
es_ES.UTF-8,fr,fr_FR,fr_FR.UTF-8,hi,hi_IN,hi_IN.UTF-8,it,it_IT,it_IT.UTF-8,ja,ja_JP,ja_JP.UTF-8,\
ko,ko_KR,ko_KR.UTF-8,nl,nl_NL,nl_NL.UTF-8,pl,pl_PL,pl_PL.UTF-8,pt,pt_BR,pt_BR.UTF-8,pt_PT,\
pt_PT.UTF-8,ru,ru_RU,ru_RU.UTF-8,sv,sv_SE,sv_SE.UTF-8,tr,tr_TR,tr_TR.UTF-8,zh,zh_CN,zh_CN.UTF-8,\
zh_TW,zh_TW.UTF-8";

/// The styles offered: no preedit and no status window, so the client draws nothing for us and
/// keeps its key handling.
pub fn input_styles() -> [InputStyle; 4] {
    [
        InputStyle::PREEDIT_NOTHING | InputStyle::STATUS_NOTHING,
        InputStyle::PREEDIT_NOTHING | InputStyle::STATUS_NONE,
        InputStyle::PREEDIT_NONE | InputStyle::STATUS_NOTHING,
        InputStyle::PREEDIT_NONE | InputStyle::STATUS_NONE,
    ]
}

/// Records focus changes; consumes nothing.
#[derive(Default)]
pub(super) struct Handler {
    pub events: Vec<ImEvent>,
}

fn key(ic: &UserInputContext<()>) -> IcKey {
    (
        ic.ic.client_win(),
        ic.ic.input_method_id().get(),
        ic.ic.input_context_id().get(),
    )
}

impl ServerHandler<XServer> for Handler {
    type InputStyleArray = [InputStyle; 4];
    type InputContextData = ();

    fn new_ic_data(&mut self, _: &mut XServer, _: InputStyle) -> Result<(), ServerError> {
        Ok(())
    }
    fn input_styles(&self) -> Self::InputStyleArray {
        input_styles()
    }
    fn filter_events(&self) -> u32 {
        0
    }
    fn handle_connect(&mut self, _: &mut XServer) -> Result<(), ServerError> {
        self.events.push(ImEvent::Connected);
        Ok(())
    }
    fn handle_create_ic(
        &mut self,
        server: &mut XServer,
        ic: &mut UserInputContext<()>,
    ) -> Result<(), ServerError> {
        // forward nothing (and nothing synchronously): keys never pass through the server
        server.set_event_mask(&ic.ic, 0, 0)
    }
    fn handle_destroy_ic(
        &mut self,
        _: &mut XServer,
        ic: UserInputContext<()>,
    ) -> Result<(), ServerError> {
        self.events.push(ImEvent::Destroyed(key(&ic)));
        Ok(())
    }
    fn handle_reset_ic(
        &mut self,
        _: &mut XServer,
        _: &mut UserInputContext<()>,
    ) -> Result<String, ServerError> {
        Ok(String::new())
    }
    fn handle_set_focus(
        &mut self,
        _: &mut XServer,
        ic: &mut UserInputContext<()>,
    ) -> Result<(), ServerError> {
        self.events.push(ImEvent::FocusIn(key(ic)));
        Ok(())
    }
    fn handle_unset_focus(
        &mut self,
        _: &mut XServer,
        ic: &mut UserInputContext<()>,
    ) -> Result<(), ServerError> {
        self.events.push(ImEvent::FocusOut(key(ic)));
        Ok(())
    }
    fn handle_set_ic_values(
        &mut self,
        _: &mut XServer,
        _: &mut UserInputContext<()>,
    ) -> Result<(), ServerError> {
        Ok(())
    }
    fn handle_forward_event(
        &mut self,
        _: &mut XServer,
        _: &mut UserInputContext<()>,
        _: &<XServer as Server>::XEvent,
    ) -> Result<bool, ServerError> {
        // never consume: the event goes back to the client unchanged
        Ok(false)
    }
}
