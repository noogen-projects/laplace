use std::fmt;
use std::path::{Path, PathBuf};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use super::Permission;

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ApplicationSettings {
    pub title: String,
    pub enabled: bool,
    pub autoload: bool,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub access_token: Option<String>,
    pub additional_static_dirs: Vec<PathBuf>,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("data")
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PermissionsSettings {
    pub required: Vec<Permission>,
    pub allowed: Vec<Permission>,
}

impl PermissionsSettings {
    pub fn is_allowed(&self, permission: Permission) -> bool {
        self.allowed.contains(&permission)
    }

    pub fn allow(&mut self, permission: Permission) -> bool {
        if !self.is_allowed(permission) {
            self.allowed.push(permission);
            true
        } else {
            false
        }
    }

    pub fn deny(&mut self, permission: Permission) -> bool {
        let index = self.allowed.iter().position(|allowed| *allowed == permission);
        if let Some(index) = index {
            self.allowed.remove(index);
            true
        } else {
            false
        }
    }

    pub fn required(&self) -> impl Iterator<Item = Permission> + '_ {
        self.required.iter().copied()
    }

    pub fn allowed(&self) -> impl Iterator<Item = Permission> + '_ {
        self.allowed.iter().copied()
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DatabaseSettings {
    pub path: Option<PathBuf>,
}

impl DatabaseSettings {
    pub const fn new() -> Self {
        Self { path: None }
    }

    pub fn path(&self) -> &Path {
        self.path.as_deref().unwrap_or_else(|| Path::new(""))
    }

    pub fn into_path(self) -> PathBuf {
        self.path.unwrap_or_default()
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct NetworkSettings {
    pub http: Option<HttpSettings>,
    pub gossipsub: Option<GossipsubSettings>,
}

impl NetworkSettings {
    pub const fn new() -> Self {
        Self {
            http: None,
            gossipsub: None,
        }
    }

    pub fn http(&self) -> &HttpSettings {
        static DEFAULT: HttpSettings = HttpSettings::new();

        self.http.as_ref().unwrap_or(&DEFAULT)
    }

    pub fn into_http(self) -> HttpSettings {
        self.http.unwrap_or_default()
    }

    pub fn gossipsub(&self) -> &GossipsubSettings {
        static DEFAULT: GossipsubSettings = GossipsubSettings::new();

        self.gossipsub.as_ref().unwrap_or(&DEFAULT)
    }

    pub fn into_gossipsub(self) -> GossipsubSettings {
        self.gossipsub.unwrap_or_default()
    }
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

impl HttpSettings {
    pub const fn new() -> Self {
        Self {
            methods: HttpMethods::new(),
            hosts: HttpHosts::new(),
            timeout_ms: http_timeout_ms(),
        }
    }
}

impl Default for HttpSettings {
    fn default() -> Self {
        Self::new()
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

impl HttpMethods {
    pub const fn new() -> Self {
        Self::All
    }
}

impl Default for HttpMethods {
    fn default() -> Self {
        Self::new()
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
            value => Err(E::custom(format!("Unknown string value: {value}"))),
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

impl HttpHosts {
    pub const fn new() -> Self {
        Self::All
    }
}

impl Default for HttpHosts {
    fn default() -> Self {
        Self::new()
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
            value => Err(E::custom(format!("Unknown string value: {value}"))),
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

impl GossipsubSettings {
    pub const fn new() -> Self {
        Self {
            addr: String::new(),
            dial_ports: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LappIncomingRequestSettings {
    pub methods: HttpMethods,
    pub request: String,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LappOutgoingRequestSettings {
    pub methods: HttpMethods,
    pub request: String,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LappRequestsSettings {
    pub lapp_name: String,
    pub incoming: Option<Vec<LappIncomingRequestSettings>>,
    pub outgoing: Option<Vec<LappOutgoingRequestSettings>>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LappSettings {
    #[serde(default)]
    pub lapp_name: String,
    pub application: ApplicationSettings,
    pub permissions: PermissionsSettings,
    pub database: Option<DatabaseSettings>,
    pub network: Option<NetworkSettings>,
    pub lapp_requests: Option<Vec<LappRequestsSettings>>,
}

impl LappSettings {
    #[inline]
    pub fn name(&self) -> &str {
        &self.lapp_name
    }

    #[inline]
    pub fn title(&self) -> &str {
        &self.application.title
    }

    #[inline]
    pub fn enabled(&self) -> bool {
        self.application.enabled
    }

    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.application.enabled = enabled;
    }

    #[inline]
    pub fn switch_enabled(&mut self) {
        self.set_enabled(!self.enabled());
    }

    #[inline]
    pub fn autoload(&self) -> bool {
        self.application.autoload
    }

    #[inline]
    pub fn set_autoload(&mut self, autoload: bool) {
        self.application.autoload = autoload;
    }

    #[inline]
    pub fn switch_autoload(&mut self) {
        self.set_autoload(!self.autoload());
    }

    pub fn database(&self) -> &DatabaseSettings {
        static DEFAULT: DatabaseSettings = DatabaseSettings::new();

        self.database.as_ref().unwrap_or(&DEFAULT)
    }

    pub fn into_database(self) -> DatabaseSettings {
        self.database.unwrap_or_default()
    }

    pub fn network(&self) -> &NetworkSettings {
        static DEFAULT: NetworkSettings = NetworkSettings::new();

        self.network.as_ref().unwrap_or(&DEFAULT)
    }

    pub fn into_network(self) -> NetworkSettings {
        self.network.unwrap_or_default()
    }

    pub fn lapp_requests(&self) -> &[LappRequestsSettings] {
        static DEFAULT: Vec<LappRequestsSettings> = Vec::new();

        self.lapp_requests.as_deref().unwrap_or(&DEFAULT)
    }

    pub fn into_lapp_requests(self) -> Vec<LappRequestsSettings> {
        self.lapp_requests.unwrap_or_default()
    }
}
