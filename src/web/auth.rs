use actix_web::{
    body::EitherBody,
    cookie::{Cookie, SameSite},
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage, HttpResponse,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::future::LocalBoxFuture;
use std::collections::HashSet;
use std::future::{ready, Ready};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const AUTH_COOKIE_NAME: &str = "synbot_auth";

fn decode_basic_credentials(value: &str) -> Option<(String, String)> {
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = BASE64.decode(encoded).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (username, password) = decoded.split_once(':')?;
    Some((username.to_string(), password.to_string()))
}

use crate::config::WebAuthConfig;
use crate::web::handlers::api::ErrorResponse;

/// Environment variable the desktop shell fills with a per-launch secret when it spawns the
/// daemon. It never reaches the config file, so it cannot be committed or copied to another box.
pub const DESKTOP_TOKEN_ENV: &str = "SYNBOT_DESKTOP_TOKEN";
/// Username reported for token-authenticated requests, so handlers can tell the desktop UI apart.
pub const DESKTOP_USER: &str = "desktop";
/// Shortest acceptable desktop secret (the shell mints 64 hex chars).
const MIN_DESKTOP_TOKEN_LEN: usize = 32;

/// Only 64-hex style secrets are honoured: a typo ("1") in the environment must not turn into a
/// one-character password for the whole dashboard.
fn usable_desktop_token(token: Option<String>) -> Option<String> {
    token.filter(|token| {
        token.len() >= MIN_DESKTOP_TOKEN_LEN && token.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// `Authorization: Bearer <desktop token>` — what the bundled page sends on every REST call.
fn bearer_token(req: &actix_web::HttpRequest) -> Option<&str> {
    let value = req.headers().get("Authorization")?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

/// Browser WebSocket handshakes cannot carry an Authorization header, so the desktop page appends
/// the same secret as `?token=`. It is honoured on `/ws/*` only, which keeps the secret out of the
/// query strings of ordinary API calls (and out of their logs).
fn ws_query_token(req: &actix_web::HttpRequest) -> Option<&str> {
    if !req.path().starts_with("/ws/") {
        return None;
    }
    req.query_string().split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "token" && !value.is_empty()).then_some(value)
    })
}

/// Compares without an early exit, so the secret cannot be recovered byte by byte from timing.
fn constant_time_eq(presented: &str, expected: &str) -> bool {
    let (presented, expected) = (presented.as_bytes(), expected.as_bytes());
    if presented.len() != expected.len() {
        return false;
    }
    presented
        .iter()
        .zip(expected)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// Middleware factory for Basic Authentication
#[derive(Clone)]
pub struct BasicAuth {
    config: Option<WebAuthConfig>,
    sessions: Arc<Mutex<HashSet<String>>>,
    /// Desktop-only bearer secret; `None` for the plain web dashboard.
    desktop_token: Option<String>,
}

impl BasicAuth {
    pub fn new(config: Option<WebAuthConfig>) -> Self {
        Self::with_desktop_token(config, None)
    }

    /// Builds the middleware with an optional desktop secret, as read from [`DESKTOP_TOKEN_ENV`].
    pub fn with_desktop_token(config: Option<WebAuthConfig>, token: Option<String>) -> Self {
        Self {
            config,
            sessions: Arc::new(Mutex::new(HashSet::new())),
            desktop_token: usable_desktop_token(token),
        }
    }

    /// Wires up the desktop secret from the process environment (the desktop shell sets it).
    pub fn from_env(config: Option<WebAuthConfig>) -> Self {
        Self::with_desktop_token(config, std::env::var(DESKTOP_TOKEN_ENV).ok())
    }

    /// True when the desktop shell handed this process a usable secret.
    pub fn desktop_enabled(&self) -> bool {
        self.desktop_token.is_some()
    }
}

impl<S, B> Transform<S, ServiceRequest> for BasicAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = BasicAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(BasicAuthMiddleware {
            service: Rc::new(service),
            config: self.config.clone(),
            sessions: self.sessions.clone(),
            desktop_token: self.desktop_token.clone(),
        }))
    }
}

pub struct BasicAuthMiddleware<S> {
    service: Rc<S>,
    config: Option<WebAuthConfig>,
    sessions: Arc<Mutex<HashSet<String>>>,
    desktop_token: Option<String>,
}

impl<S, B> Service<ServiceRequest> for BasicAuthMiddleware<S>
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
        let config = self.config.clone();
        let sessions = self.sessions.clone();
        let desktop_token = self.desktop_token.clone();

        Box::pin(async move {
            // If auth is not configured, allow all requests.
            let Some(auth_config) = config else {
                return service
                    .call(req)
                    .await
                    .map(ServiceResponse::map_into_left_body);
            };

            // The desktop shell's secret wins over the password: the bundled UI never sees the
            // latter, so a lost laptop does not leak the web dashboard credentials.
            let desktop_authorized = desktop_token.as_deref().is_some_and(|expected| {
                bearer_token(req.request())
                    .is_some_and(|presented| constant_time_eq(presented, expected))
                    || ws_query_token(req.request())
                        .is_some_and(|presented| constant_time_eq(presented, expected))
            });

            if desktop_authorized {
                req.extensions_mut().insert(AuthenticatedUser {
                    username: DESKTOP_USER.to_string(),
                });
                return service
                    .call(req)
                    .await
                    .map(ServiceResponse::map_into_left_body);
            }

            let basic_authorized = req
                .headers()
                .get("Authorization")
                .and_then(|value| value.to_str().ok())
                .and_then(decode_basic_credentials)
                .is_some_and(|(username, password)| {
                    username == auth_config.username && password == auth_config.password
                });

            // Native browser WebSocket clients cannot set Authorization headers, but they do
            // send same-origin HttpOnly cookies during the upgrade request.
            let session_authorized = req
                .cookie(AUTH_COOKIE_NAME)
                .is_some_and(|cookie| sessions.lock().unwrap().contains(cookie.value()));

            if basic_authorized || session_authorized {
                req.extensions_mut().insert(AuthenticatedUser {
                    username: auth_config.username.clone(),
                });

                let session_token = if basic_authorized {
                    let token = Uuid::new_v4().to_string();
                    sessions.lock().unwrap().insert(token.clone());
                    Some(token)
                } else {
                    None
                };

                let response = service.call(req).await?;
                let mut response = response.map_into_left_body();

                if let Some(token) = session_token {
                    let mut cookie = Cookie::new(AUTH_COOKIE_NAME, token);
                    cookie.set_path("/");
                    cookie.set_http_only(true);
                    cookie.set_same_site(Some(SameSite::Lax));
                    response
                        .response_mut()
                        .add_cookie(&cookie)
                        .map_err(|error| {
                            actix_web::error::ErrorInternalServerError(format!(
                                "Failed to set authentication cookie: {error}"
                            ))
                        })?;
                }

                Ok(response)
            } else {
                // Return 401 for every protected route, including WebSocket upgrades.
                let error_response = ErrorResponse::new(
                    "Authentication required".to_string(),
                    "UNAUTHORIZED".to_string(),
                );

                let (req, _) = req.into_parts();
                let response = HttpResponse::Unauthorized()
                    .insert_header(("WWW-Authenticate", "Basic realm=\"Web Admin Dashboard\""))
                    .json(error_response)
                    .map_into_right_body();

                Ok(ServiceResponse::new(req, response))
            }
        })
    }
}

/// Authenticated user information stored in request extensions
#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub username: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App, HttpResponse};

    async fn test_handler() -> HttpResponse {
        HttpResponse::Ok().body("success")
    }

    #[actix_web::test]
    async fn test_no_auth_config_allows_all() {
        let app = test::init_service(
            App::new()
                .wrap(BasicAuth::new(None))
                .route("/test", web::get().to(test_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/test").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_valid_credentials_allowed() {
        let auth_config = WebAuthConfig {
            username: "admin".to_string(),
            password: "secret".to_string(),
        };

        let app = test::init_service(
            App::new()
                .wrap(BasicAuth::new(Some(auth_config)))
                .route("/test", web::get().to(test_handler)),
        )
        .await;

        // Create valid Basic Auth header: "admin:secret" -> base64
        let credentials = BASE64.encode("admin:secret");
        let auth_header = format!("Basic {}", credentials);

        let req = test::TestRequest::get()
            .uri("/test")
            .insert_header(("Authorization", auth_header))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_invalid_credentials_rejected() {
        let auth_config = WebAuthConfig {
            username: "admin".to_string(),
            password: "secret".to_string(),
        };

        let app = test::init_service(
            App::new()
                .wrap(BasicAuth::new(Some(auth_config)))
                .route("/test", web::get().to(test_handler)),
        )
        .await;

        // Create invalid Basic Auth header: "admin:wrong" -> base64
        let credentials = BASE64.encode("admin:wrong");
        let auth_header = format!("Basic {}", credentials);

        let req = test::TestRequest::get()
            .uri("/test")
            .insert_header(("Authorization", auth_header))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn test_missing_auth_header_rejected() {
        let auth_config = WebAuthConfig {
            username: "admin".to_string(),
            password: "secret".to_string(),
        };

        let app = test::init_service(
            App::new()
                .wrap(BasicAuth::new(Some(auth_config)))
                .route("/test", web::get().to(test_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/test").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn test_malformed_auth_header_rejected() {
        let auth_config = WebAuthConfig {
            username: "admin".to_string(),
            password: "secret".to_string(),
        };

        let app = test::init_service(
            App::new()
                .wrap(BasicAuth::new(Some(auth_config)))
                .route("/test", web::get().to(test_handler)),
        )
        .await;

        // Test various malformed headers
        let test_cases = vec![
            "Bearer token123",         // Wrong auth type
            "Basic invalid-base64!@#", // Invalid base64
            "Basic YWRtaW4=",          // Valid base64 but no colon separator
        ];

        for auth_header in test_cases {
            let req = test::TestRequest::get()
                .uri("/test")
                .insert_header(("Authorization", auth_header))
                .to_request();

            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 401, "Failed for header: {}", auth_header);
        }
    }

    #[actix_web::test]
    async fn test_www_authenticate_header_present() {
        let auth_config = WebAuthConfig {
            username: "admin".to_string(),
            password: "secret".to_string(),
        };

        let app = test::init_service(
            App::new()
                .wrap(BasicAuth::new(Some(auth_config)))
                .route("/test", web::get().to(test_handler)),
        )
        .await;

        let req = test::TestRequest::get().uri("/test").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 401);
        assert!(resp.headers().contains_key("WWW-Authenticate"));
    }

    const DESKTOP_SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn password_config() -> WebAuthConfig {
        WebAuthConfig {
            username: "admin".to_string(),
            password: "secret".to_string(),
        }
    }

    #[actix_web::test]
    async fn test_desktop_bearer_token_is_accepted() {
        let app = test::init_service(
            App::new()
                .wrap(BasicAuth::with_desktop_token(
                    Some(password_config()),
                    Some(DESKTOP_SECRET.to_string()),
                ))
                .route("/test", web::get().to(test_handler)),
        )
        .await;

        let authorized = test::TestRequest::get()
            .uri("/test")
            .insert_header(("Authorization", format!("Bearer {DESKTOP_SECRET}")))
            .to_request();
        assert_eq!(test::call_service(&app, authorized).await.status(), 200);

        let wrong = test::TestRequest::get()
            .uri("/test")
            .insert_header(("Authorization", format!("Bearer {}", "0".repeat(64))))
            .to_request();
        assert_eq!(test::call_service(&app, wrong).await.status(), 401);
    }

    #[actix_web::test]
    async fn test_desktop_query_token_is_only_honoured_on_websockets() {
        let app = test::init_service(
            App::new()
                .wrap(BasicAuth::with_desktop_token(
                    Some(password_config()),
                    Some(DESKTOP_SECRET.to_string()),
                ))
                .route("/ws/chat", web::get().to(test_handler))
                .route("/api/status", web::get().to(test_handler)),
        )
        .await;

        // Browsers cannot set headers on a WebSocket handshake, so the query string carries it.
        let handshake = test::TestRequest::get()
            .uri(&format!("/ws/chat?token={DESKTOP_SECRET}"))
            .to_request();
        assert_eq!(test::call_service(&app, handshake).await.status(), 200);

        // Everywhere else the secret stays in the header, so it cannot end up in access logs.
        let rest = test::TestRequest::get()
            .uri(&format!("/api/status?token={DESKTOP_SECRET}"))
            .to_request();
        assert_eq!(test::call_service(&app, rest).await.status(), 401);
    }

    #[actix_web::test]
    async fn test_short_desktop_tokens_are_ignored() {
        let auth = BasicAuth::with_desktop_token(Some(password_config()), Some("abc".to_string()));
        assert!(
            !auth.desktop_enabled(),
            "a short secret must not weaken the password"
        );

        let app = test::init_service(
            App::new()
                .wrap(auth)
                .route("/test", web::get().to(test_handler)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/test")
            .insert_header(("Authorization", "Bearer abc"))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 401);
    }

    #[actix_web::test]
    async fn test_desktop_token_does_not_disable_the_password() {
        let app = test::init_service(
            App::new()
                .wrap(BasicAuth::with_desktop_token(
                    Some(password_config()),
                    Some(DESKTOP_SECRET.to_string()),
                ))
                .route("/test", web::get().to(test_handler)),
        )
        .await;

        let credentials = BASE64.encode("admin:secret");
        let req = test::TestRequest::get()
            .uri("/test")
            .insert_header(("Authorization", format!("Basic {credentials}")))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 200);
    }

    // `use actix_web::test` above shadows the built-in `#[test]` attribute in this module, so even
    // a synchronous assertion has to go through the actix macro.
    #[actix_web::test]
    async fn test_constant_time_eq_matches_only_identical_secrets() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("", "a"));
    }
}
