use std::{marker::PhantomData, ops::Deref};

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::lapp::{Lapp, Permission};

#[skip_serializing_none]
#[derive(Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct UpdateQuery {
    pub lapp_name: String,
    pub enabled: Option<bool>,
    pub allow_permission: Option<Permission>,
    pub deny_permission: Option<Permission>,
}

impl UpdateQuery {
    pub fn new(lapp_name: impl Into<String>) -> Self {
        Self {
            lapp_name: lapp_name.into(),
            ..Default::default()
        }
    }

    pub fn enabled(mut self, enabled: impl Into<Option<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    pub fn allow_permission(mut self, allow_permission: impl Into<Option<Permission>>) -> Self {
        self.allow_permission = allow_permission.into();
        self
    }

    pub fn deny_permission(mut self, deny_permission: impl Into<Option<Permission>>) -> Self {
        self.deny_permission = deny_permission.into();
        self
    }

    pub fn update_permission(self, permission: impl Into<Permission>, allow: bool) -> Self {
        if allow {
            self.allow_permission(permission.into())
        } else {
            self.deny_permission(permission.into())
        }
    }

    pub fn into_request(self) -> UpdateRequest {
        self.into()
    }

    pub fn into_response<'a, PathT: Clone, LappT: Deref<Target = Lapp<PathT>>>(self) -> Response<'a, PathT, LappT> {
        self.into()
    }
}

impl From<UpdateRequest> for UpdateQuery {
    fn from(request: UpdateRequest) -> Self {
        request.update
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct UpdateRequest {
    pub update: UpdateQuery,
}

impl UpdateRequest {
    pub fn into_query(self) -> UpdateQuery {
        self.into()
    }
}

impl From<UpdateQuery> for UpdateRequest {
    fn from(update: UpdateQuery) -> Self {
        Self { update }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Response<'a, PathT: Clone, LappT: Deref<Target = Lapp<PathT>> + 'a> {
    Lapps {
        lapps: Vec<LappT>,

        #[serde(skip)]
        _marker: PhantomData<&'a Lapp<PathT>>,
    },

    Updated {
        updated: UpdateQuery,
    },
}

impl<'a, PathT: Clone, LappT: Deref<Target = Lapp<PathT>> + 'a> Response<'a, PathT, LappT> {
    pub fn lapps(lapps: impl Into<Vec<LappT>>) -> Self {
        Self::Lapps {
            lapps: lapps.into(),
            _marker: Default::default(),
        }
    }
}

impl<'a, PathT: Clone, LappT: Deref<Target = Lapp<PathT>> + 'a> From<Vec<LappT>> for Response<'a, PathT, LappT> {
    fn from(lapps: Vec<LappT>) -> Self {
        Self::Lapps {
            lapps,
            _marker: Default::default(),
        }
    }
}

impl<'a, PathT: Clone, LappT: Deref<Target = Lapp<PathT>> + 'a> From<UpdateQuery> for Response<'a, PathT, LappT> {
    fn from(updated: UpdateQuery) -> Self {
        Self::Updated { updated }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_request() {
        let request = UpdateQuery::new("test").into_request();
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"update":{"lapp_name":"test"}}"#);

        let request = UpdateQuery::new("test").enabled(true).into_request();
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"update":{"lapp_name":"test","enabled":true}}"#);

        let request = UpdateQuery::new("test")
            .enabled(true)
            .allow_permission(Permission::Http)
            .deny_permission(Permission::Tcp)
            .into_request();
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"update":{"lapp_name":"test","enabled":true,"allow_permission":"http","deny_permission":"tcp"}}"#
        );
    }

    #[test]
    fn deserialize_request() {
        let json = r#"{"update":{"lapp_name":"test"}}"#;
        let request: UpdateRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request, UpdateRequest {
            update: UpdateQuery {
                lapp_name: "test".to_string(),
                ..Default::default()
            }
        });
    }

    #[test]
    fn serialize_lapps_response() {
        let response = Response::<'_, String, &Lapp<String>>::from(vec![]);
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"lapps":[]}"#);
    }

    #[test]
    fn serialize_updated_response() {
        let response = Response::Updated::<'_, String, &Lapp<String>> {
            updated: UpdateQuery::new("test"),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"updated":{"lapp_name":"test"}}"#);

        let response = Response::Updated::<'_, String, &Lapp<String>> {
            updated: UpdateQuery::new("test").enabled(true),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"updated":{"lapp_name":"test","enabled":true}}"#);

        let response = Response::Updated::<'_, String, &Lapp<String>> {
            updated: UpdateQuery::new("test")
                .enabled(true)
                .allow_permission(Permission::Http)
                .deny_permission(Permission::Tcp),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            json,
            r#"{"updated":{"lapp_name":"test","enabled":true,"allow_permission":"http","deny_permission":"tcp"}}"#
        );
    }
}
