use anyhow::{Error, Result};
use yew::html::Scope;
use yew::Component;

pub trait MsgError {
    type Map;

    fn msg_error<Comp>(self, link: &Scope<Comp>)
    where
        Comp: Component,
        Comp::Message: From<Error>;

    fn msg_error_map<Comp>(self, link: &Scope<Comp>) -> Self::Map
    where
        Comp: Component,
        Comp::Message: From<Error>;
}

impl<T> MsgError for Result<T> {
    type Map = std::result::Result<T, ()>;

    fn msg_error<Comp>(self, link: &Scope<Comp>)
    where
        Comp: Component,
        Comp::Message: From<Error>,
    {
        if let Err(err) = self {
            link.send_message(Comp::Message::from(err))
        }
    }

    fn msg_error_map<Comp>(self, link: &Scope<Comp>) -> Self::Map
    where
        Comp: Component,
        <Comp as Component>::Message: From<Error>,
    {
        self.map_err(|err| link.send_message(Comp::Message::from(err)))
    }
}
