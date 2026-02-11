use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage, HttpResponse,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};
use std::rc::Rc;

use crate::config::WebAuthConfig;
use crate::web::handlers::api::ErrorResponse;

/// Middleware factory for Basic Authentication
#[derive(Clone)]
pub struct BasicAuth {
    config: Option<WebAuthConfig>,
}

impl BasicAuth {
    pub fn new(config: Option<WebAuthConfig>) -> Self {
        Self { config }
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
        }))
    }
}

pub struct BasicAuthMiddleware<S> {
    service: Rc<S>,
    config: Option<WebAuthConfig>,
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

        Box::pin(async move {
            // If auth is not configured, allow all requests
            let Some(auth_config) = config else {
                return service.call(req).await.map(ServiceResponse::map_into_left_body);
            };

            // Check if the request has Authorization header
            let auth_header = req.headers().get("Authorization");

            let authorized = if let Some(auth_value) = auth_header {
                // Parse Basic Auth header
                if let Ok(auth_str) = auth_value.to_str() {
                    if let Some(credentials) = auth_str.strip_prefix("Basic ") {
                        // Decode base64 credentials
                        if let Ok(decoded) = BASE64.decode(credentials) {
                            if let Ok(decoded_str) = String::from_utf8(decoded) {
                                // Split username:password
                                if let Some((username, password)) = decoded_str.split_once(':') {
                                    // Verify credentials
                                    username == auth_config.username
                                        && password == auth_config.password
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if authorized {
                // Store authenticated user in request extensions
                req.extensions_mut().insert(AuthenticatedUser {
                    username: auth_config.username.clone(),
                });
                service.call(req).await.map(ServiceResponse::map_into_left_body)
            } else {
                // Return 401 Unauthorized
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
            "Bearer token123",           // Wrong auth type
            "Basic invalid-base64!@#",   // Invalid base64
            "Basic YWRtaW4=",            // Valid base64 but no colon separator
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
}
