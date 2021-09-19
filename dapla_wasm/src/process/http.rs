pub use http as types;
pub use http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, Version};

pub type Request = http::Request<Vec<u8>>;
pub type Response = http::Response<Vec<u8>>;
