use actix_web::{HttpResponse, Responder};

// Conditional compilation: only embed assets if the dist folder exists
#[cfg(not(debug_assertions))]
use rust_embed::RustEmbed;

#[cfg(not(debug_assertions))]
#[derive(RustEmbed)]
#[folder = "web-dashboard/dist"]
#[prefix = ""]
pub struct Assets;

/// Serve static files from embedded assets
/// 
/// This handler serves files from the embedded frontend build.
/// If a file is not found, it falls back to serving index.html
/// to support SPA client-side routing.
pub async fn serve_static(_path: actix_web::web::Path<String>) -> impl Responder {
    #[cfg(not(debug_assertions))]
    {
        let path = path.into_inner();
        
        // Default to index.html for root path
        let file_path = if path.is_empty() || path == "/" {
            "index.html"
        } else {
            // Remove leading slash if present
            path.strip_prefix('/').unwrap_or(&path)
        };
        
        match Assets::get(file_path) {
            Some(content) => {
                // Determine MIME type from file extension
                let mime = mime_guess::from_path(file_path)
                    .first_or_octet_stream();
                
                HttpResponse::Ok()
                    .content_type(mime.as_ref())
                    .body(content.data.into_owned())
            }
            None => {
                // SPA fallback: return index.html for non-API routes
                // This enables client-side routing to work
                if let Some(index) = Assets::get("index.html") {
                    HttpResponse::Ok()
                        .content_type("text/html")
                        .body(index.data.into_owned())
                } else {
                    HttpResponse::NotFound()
                        .body("404 Not Found")
                }
            }
        }
    }
    
    #[cfg(debug_assertions)]
    {
        // In debug mode, return a placeholder message
        HttpResponse::Ok()
            .content_type("text/html")
            .body("<html><body><h1>Frontend not built</h1><p>Run the frontend build to see the dashboard.</p></body></html>")
    }
}

/// Serve index.html for the root path
pub async fn serve_index() -> impl Responder {
    #[cfg(not(debug_assertions))]
    {
        match Assets::get("index.html") {
            Some(content) => HttpResponse::Ok()
                .content_type("text/html")
                .body(content.data.into_owned()),
            None => HttpResponse::NotFound()
                .body("404 Not Found - index.html not found in embedded assets"),
        }
    }
    
    #[cfg(debug_assertions)]
    {
        // In debug mode, return a placeholder message
        HttpResponse::Ok()
            .content_type("text/html")
            .body("<html><body><h1>Web Admin Dashboard</h1><p>Frontend not built. Run the frontend build to see the dashboard.</p></body></html>")
    }
}

