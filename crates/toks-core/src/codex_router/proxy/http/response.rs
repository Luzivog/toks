use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Response, StatusCode};

pub(super) fn build_response(status: StatusCode, headers: HeaderMap, body: Body) -> Response<Body> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

pub(super) fn body_is_identity_encoded(headers: &HeaderMap) -> bool {
    let mut encoding = None;
    for value in headers.get_all(header::CONTENT_ENCODING) {
        let Ok(value) = value.to_str() else {
            return false;
        };
        for token in value.split(',') {
            let token = token.trim();
            if token.is_empty() || encoding.is_some() || !token.eq_ignore_ascii_case("identity") {
                return false;
            }
            encoding = Some(());
        }
    }
    true
}

pub(super) fn plain(status: StatusCode, message: &'static str) -> Response<Body> {
    build_response(status, Default::default(), Body::from(message))
}

pub(super) fn usage_unavailable() -> Response<Body> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    build_response(
        StatusCode::TOO_MANY_REQUESTS,
        headers,
        Body::from(super::super::protocol::ALL_UNAVAILABLE_FRAME),
    )
}
