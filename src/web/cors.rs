use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::header::{self, HeaderValue},
    Error, HttpResponse,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};
use std::rc::Rc;

/// CORS middleware configuration
#[derive(Clone)]
pub struct Cors {
    allowed_origins: Vec<String>,
}

impl Cors {
    /// Create a new CORS middleware with the specified allowed origins
    pub fn new(allowed_origins: Vec<String>) -> Self {
        Self { allowed_origins }
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
        }))
    }
}

pub struct CorsMiddleware<S> {
    service: Rc<S>,
    allowed_origins: Vec<String>,
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
                    .map(|o| {
                        allowed_origins.is_empty()
                            || allowed_origins.contains(&"*".to_string())
                            || allowed_origins.contains(o)
                    })
                    .unwrap_or(false);

                if is_allowed {
                    let mut response = HttpResponse::Ok();

                    if let Some(origin) = origin {
                        if allowed_origins.contains(&"*".to_string()) {
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
                let is_allowed = allowed_origins.is_empty()
                    || allowed_origins.contains(&"*".to_string())
                    || allowed_origins.contains(&origin);

                if is_allowed {
                    let headers = res.headers_mut();

                    if allowed_origins.contains(&"*".to_string()) {
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
