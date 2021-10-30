use std::{fmt, path::PathBuf};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use super::Permission;

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ApplicationSettings {
    pub title: String,
    pub enabled: bool,
    pub access_token: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PermissionsSettings {
    pub required: Vec<Permission>,
    pub allowed: Vec<Permission>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DatabaseSettings {
    pub path: PathBuf,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct NetworkSettings {
    pub http: HttpSettings,
    pub gossipsub: GossipsubSettings,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct HttpSettings {
    pub methods: HttpMethods,
    pub hosts: HttpHosts,
    #[serde(default = "http_timeout_ms")]
    pub timeout_ms: u64,
}

const fn http_timeout_ms() -> u64 {
    1000 * 10
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self {
            methods: Default::default(),
            hosts: Default::default(),
            timeout_ms: http_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub enum HttpMethods {
    All,
    List(Vec<HttpMethod>),
}

impl Default for HttpMethods {
    fn default() -> Self {
        Self::All
    }
}

impl Serialize for HttpMethods {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::All => serializer.serialize_str("all"),
            Self::List(methods) => methods.serialize(serializer),
        }
    }
}

struct HttpMethodsVisitor;

impl<'de> de::Visitor<'de> for HttpMethodsVisitor {
    type Value = HttpMethods;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an array of HttpMethod or string \"all\".")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value.to_lowercase().as_str() {
            "all" => Ok(HttpMethods::All),
            value => Err(E::custom(format!("Unknown string value: {}", value))),
        }
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut methods = Vec::new();
        while let Some(method) = sequence.next_element()? {
            methods.push(method);
        }
        Ok(HttpMethods::List(methods))
    }
}

impl<'de> Deserialize<'de> for HttpMethods {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(HttpMethodsVisitor)
    }
}

#[derive(Debug, Clone)]
pub enum HttpHosts {
    All,
    List(Vec<String>),
}

impl Default for HttpHosts {
    fn default() -> Self {
        Self::All
    }
}

impl Serialize for HttpHosts {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::All => serializer.serialize_str("all"),
            Self::List(hosts) => hosts.serialize(serializer),
        }
    }
}

struct HttpHostsVisitor;

impl<'de> de::Visitor<'de> for HttpHostsVisitor {
    type Value = HttpHosts;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an array of HttpHost or string \"all\".")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value.to_lowercase().as_str() {
            "all" => Ok(HttpHosts::All),
            value => Err(E::custom(format!("Unknown string value: {}", value))),
        }
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let mut methods = Vec::new();
        while let Some(method) = sequence.next_element()? {
            methods.push(method);
        }
        Ok(HttpHosts::List(methods))
    }
}

impl<'de> Deserialize<'de> for HttpHosts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(HttpHostsVisitor)
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct GossipsubSettings {
    pub addr: String,
    pub dial_ports: Vec<u16>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DapIncomingRequestSettings {
    pub methods: HttpMethods,
    pub request: String,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DapOutgoingRequestSettings {
    pub methods: HttpMethods,
    pub request: String,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DapRequestsSettings {
    pub dap_name: String,
    pub incoming: Vec<DapIncomingRequestSettings>,
    pub outgoing: Vec<DapOutgoingRequestSettings>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DapSettings {
    pub application: ApplicationSettings,
    pub permissions: PermissionsSettings,
    pub database: DatabaseSettings,
    pub network: NetworkSettings,
    pub dap_requests: Vec<DapRequestsSettings>,
}
