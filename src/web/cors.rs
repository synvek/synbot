use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::header::{self, HeaderValue},
    Error, HttpResponse,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};
use std::rc::Rc;

/// Origins the Tauri shell can present. macOS/Linux use a custom scheme, Windows uses an
/// `http://` host; all three are cross-origin from the loopback API's point of view.
pub const DESKTOP_ORIGINS: [&str; 3] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

/// CORS middleware configuration
#[derive(Clone)]
pub struct Cors {
    allowed_origins: Vec<String>,
    /// Empty/`*` in the config means "allow every origin"; that has to stay true *after* the
    /// desktop origins are appended, or adding them would silently turn allow-all into a
    /// two-entry whitelist.
    allow_all: bool,
}

impl Cors {
    /// Create a new CORS middleware with the specified allowed origins
    pub fn new(allowed_origins: Vec<String>) -> Self {
        let allow_all = allowed_origins.is_empty() || allowed_origins.iter().any(|o| o == "*");
        Self {
            allowed_origins,
            allow_all,
        }
    }

    /// Admits the desktop shell's application origin on top of the configured origins.
    ///
    /// Only used when the desktop shell handed the process a per-launch token: the bundled page
    /// is not served by this server, so its requests are cross-origin even though both ends are
    /// on loopback.
    pub fn allow_desktop_origins(mut self) -> Self {
        for origin in DESKTOP_ORIGINS {
            if !self.allowed_origins.iter().any(|allowed| allowed == origin) {
                self.allowed_origins.push(origin.to_string());
            }
        }
        self
    }
}

impl<S, B> Transform<S, ServiceRequest> for Cors
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = CorsMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CorsMiddleware {
            service: Rc::new(service),
            allowed_origins: self.allowed_origins.clone(),
            allow_all: self.allow_all,
        }))
    }
}

pub struct CorsMiddleware<S> {
    service: Rc<S>,
    allowed_origins: Vec<String>,
    allow_all: bool,
}

impl<S, B> Service<ServiceRequest> for CorsMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let allowed_origins = self.allowed_origins.clone();
        let allow_all = self.allow_all;

        Box::pin(async move {
            // Handle preflight OPTIONS requests
            if req.method() == actix_web::http::Method::OPTIONS {
                let origin = req
                    .headers()
                    .get(header::ORIGIN)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                let is_allowed = origin
                    .as_ref()
                    .map(|o| allow_all || allowed_origins.contains(o))
                    .unwrap_or(false);

                if is_allowed {
                    let mut response = HttpResponse::Ok();

                    if let Some(origin) = origin {
                        if allow_all {
                            response.insert_header((header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"));
                        } else {
                            response.insert_header((
                                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                                origin.as_str(),
                            ));
                        }
                    }

                    response.insert_header((
                        header::ACCESS_CONTROL_ALLOW_METHODS,
                        "GET, POST, PUT, PATCH, DELETE, OPTIONS",
                    ));
                    response.insert_header((
                        header::ACCESS_CONTROL_ALLOW_HEADERS,
                        "Content-Type, Authorization",
                    ));
                    response.insert_header((header::ACCESS_CONTROL_MAX_AGE, "3600"));

                    let (req, _) = req.into_parts();
                    let response = response.finish().map_into_right_body();
                    return Ok(ServiceResponse::new(req, response));
                }
            }

            // For non-preflight requests, add CORS headers to the response
            let origin = req
                .headers()
                .get(header::ORIGIN)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let mut res = service.call(req).await?;

            if let Some(origin) = origin {
                let is_allowed = allow_all || allowed_origins.contains(&origin);

                if is_allowed {
                    let headers = res.headers_mut();

                    if allow_all {
                        headers.insert(
                            header::ACCESS_CONTROL_ALLOW_ORIGIN,
                            HeaderValue::from_static("*"),
                        );
                    } else if let Ok(origin_value) = HeaderValue::from_str(&origin) {
                        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin_value);
                    }

                    headers.insert(
                        header::ACCESS_CONTROL_ALLOW_METHODS,
                        HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE, OPTIONS"),
                    );
                    headers.insert(
                        header::ACCESS_CONTROL_ALLOW_HEADERS,
                        HeaderValue::from_static("Content-Type, Authorization"),
                    );
                }
            }

            Ok(res.map_into_left_body())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_configuration_still_allows_every_origin_after_desktop_origins_are_added() {
        let cors = Cors::new(Vec::new()).allow_desktop_origins();
        assert!(
            cors.allow_all,
            "an empty config means allow-all, not a two-entry whitelist"
        );
        assert!(cors
            .allowed_origins
            .contains(&"tauri://localhost".to_string()));
    }

    #[test]
    fn wildcard_configuration_keeps_allowing_every_origin() {
        let cors = Cors::new(vec!["*".to_string()]).allow_desktop_origins();
        assert!(cors.allow_all);
    }

    #[test]
    fn an_explicit_whitelist_stays_a_whitelist_and_admits_the_desktop_origin() {
        let cors = Cors::new(vec!["https://ops.example.com".to_string()]).allow_desktop_origins();
        assert!(!cors.allow_all);
        assert!(cors
            .allowed_origins
            .contains(&"https://ops.example.com".to_string()));
        assert!(cors
            .allowed_origins
            .contains(&"http://tauri.localhost".to_string()));
    }

    #[test]
    fn desktop_origins_are_not_duplicated() {
        let cors = Cors::new(vec!["tauri://localhost".to_string()]).allow_desktop_origins();
        let occurrences = cors
            .allowed_origins
            .iter()
            .filter(|origin| origin.as_str() == "tauri://localhost")
            .count();
        assert_eq!(occurrences, 1);
    }
}
