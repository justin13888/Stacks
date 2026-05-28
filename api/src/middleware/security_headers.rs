use salvo::http::HeaderValue;
use salvo::prelude::*;

pub struct SecurityHeaders;

#[async_trait]
impl Handler for SecurityHeaders {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        ctrl.call_next(req, depot, res).await;
        let headers = res.headers_mut();
        // All header values here are static byte strings, so `from_static` is
        // infallible — no panics from a stray non-ASCII char.
        headers.insert(
            "X-Content-Type-Options",
            HeaderValue::from_static("nosniff"),
        );
        headers.insert("X-Frame-Options", HeaderValue::from_static("DENY"));
        headers.insert(
            "Content-Security-Policy",
            HeaderValue::from_static("default-src 'self'"),
        );
        headers.insert(
            "Referrer-Policy",
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        );
    }
}
