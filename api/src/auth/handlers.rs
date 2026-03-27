use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// Initiate OIDC login flow; returns redirect URL.
#[salvo::oapi::endpoint(tags("Auth"), summary = "Begin OIDC login")]
pub async fn oidc_login(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<OidcLoginResponse>, OidcLoginError> {
    todo!()
}

/// Handle OIDC provider callback with `code` and `state`.
#[salvo::oapi::endpoint(tags("Auth"), summary = "OIDC callback")]
pub async fn oidc_callback(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<OidcCallbackResponse>, OidcCallbackError> {
    todo!()
}

/// Exchange a refresh token for a new access/refresh token pair.
#[salvo::oapi::endpoint(tags("Auth"), summary = "Refresh access token")]
pub async fn refresh_token(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<RefreshResponse>, RefreshError> {
    todo!()
}

/// Revoke the current session / invalidate tokens.
#[salvo::oapi::endpoint(tags("Auth"), summary = "Logout")]
pub async fn logout(_req: &mut Request, _res: &mut Response) {
    todo!()
}

/// List API tokens for the authenticated user.
#[salvo::oapi::endpoint(tags("Auth"), summary = "List API tokens")]
pub async fn list_tokens(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<Vec<ApiTokenResponse>>, CreateTokenError> {
    todo!()
}

/// Create a new API token; plaintext is returned once.
#[salvo::oapi::endpoint(tags("Auth"), summary = "Create API token")]
pub async fn create_token(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<CreateApiTokenResponse>, CreateTokenError> {
    todo!()
}

/// Delete (revoke) an API token by ID.
#[salvo::oapi::endpoint(tags("Auth"), summary = "Delete API token")]
pub async fn delete_token(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<()>, DeleteTokenError> {
    todo!()
}
