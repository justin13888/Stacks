use salvo::prelude::*;
use stacks_core::auth::service::AuthService;

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

fn token_to_dto(t: stacks_core::entity::api_token::Model) -> ApiTokenResponse {
    ApiTokenResponse {
        id: t.id,
        label: t.label,
        scopes: serde_json::from_value(t.scopes).unwrap_or_default(),
        expires_at: t.expires_at.map(|dt| dt.to_rfc3339()),
        created_at: t.created_at.to_rfc3339(),
    }
}

/// Initiate OIDC login flow; returns redirect URL.
#[salvo::oapi::endpoint(tags("Auth"), summary = "Begin OIDC login")]
pub async fn oidc_login(depot: &mut Depot) -> Result<Json<OidcLoginResponse>, OidcLoginError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = AuthService::new(state.db, state.config.auth, state.config.oidc);
    let params = svc
        .oidc_login_url()
        .await
        .map_err(|e| OidcLoginError(crate::internal!(e)))?;
    Ok(Json(OidcLoginResponse {
        auth_url: params.auth_url,
    }))
}

/// Handle the OIDC provider callback.
///
/// `state` is verified against a server-side record created by `/auth/login`,
/// which also supplies the expected nonce.
#[salvo::oapi::endpoint(tags("Auth"), summary = "OIDC callback")]
pub async fn oidc_callback(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<OidcCallbackResponse>, OidcCallbackError> {
    let body: OidcCallbackRequest = req
        .parse_json()
        .await
        .map_err(|e| OidcCallbackError::Internal(crate::internal!(e)))?;

    let state_val = depot.obtain::<AppState>().unwrap().clone();
    let svc = AuthService::new(state_val.db, state_val.config.auth, state_val.config.oidc);

    let pair = svc
        .oidc_callback(&body.code, &body.state)
        .await
        .map_err(|e| match e {
            stacks_core::auth::error::AuthError::OidcError(detail) => {
                // The client only ever sees INVALID_CODE, so without this the
                // real cause — an unreachable token endpoint, a nonce mismatch,
                // an expired state — is invisible to the operator too.
                tracing::warn!(error = %detail, "OIDC callback rejected");
                OidcCallbackError::InvalidCode
            }
            stacks_core::auth::error::AuthError::UserNotProvisioned => {
                OidcCallbackError::ProvisioningFailed
            }
            e => OidcCallbackError::Internal(crate::internal!(e)),
        })?;

    Ok(Json(OidcCallbackResponse {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_in: pair.expires_in,
    }))
}

/// Exchange a refresh token for a new access/refresh token pair.
#[salvo::oapi::endpoint(tags("Auth"), summary = "Refresh access token")]
pub async fn refresh_token(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<RefreshResponse>, RefreshError> {
    let body: RefreshRequest = req.parse_json().await.into_internal()?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = AuthService::new(state.db, state.config.auth, state.config.oidc);

    let pair = svc
        .refresh(&body.refresh_token)
        .await
        .map_err(|e| match e {
            stacks_core::auth::error::AuthError::TokenExpired => RefreshError::TokenExpired,
            stacks_core::auth::error::AuthError::InvalidToken => RefreshError::InvalidToken,
            e => RefreshError::Internal(crate::internal!(e)),
        })?;

    Ok(Json(RefreshResponse {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_in: pair.expires_in,
    }))
}

/// Revoke the current session / invalidate tokens.
#[salvo::oapi::endpoint(tags("Auth"), summary = "Logout")]
pub async fn logout(depot: &mut Depot, res: &mut Response) {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = AuthService::new(state.db, state.config.auth, state.config.oidc);
    let _ = svc.logout(&user.id).await;
    res.status_code(StatusCode::NO_CONTENT);
}

/// List API tokens for the authenticated user.
#[salvo::oapi::endpoint(tags("Auth"), summary = "List API tokens")]
pub async fn list_tokens(
    depot: &mut Depot,
) -> Result<Json<Vec<ApiTokenResponse>>, CreateTokenError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = AuthService::new(state.db, state.config.auth, state.config.oidc);
    let tokens = svc
        .list_api_tokens(&user.id)
        .await
        .map_err(|e| CreateTokenError::Internal(crate::internal!(e)))?;
    Ok(Json(tokens.into_iter().map(token_to_dto).collect()))
}

/// Create a new API token; plaintext is returned once.
#[salvo::oapi::endpoint(tags("Auth"), summary = "Create API token")]
pub async fn create_token(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<CreateApiTokenResponse>, CreateTokenError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: CreateApiTokenRequest = req.parse_json().await.into_internal()?;

    let expires_at = body
        .expires_at
        .as_deref()
        .map(|s| chrono::DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&chrono::Utc)))
        .transpose()
        .into_internal()?;

    let svc = AuthService::new(state.db, state.config.auth, state.config.oidc);
    let (model, plaintext) = svc
        .create_api_token(&user.id, &body.label, body.scopes, expires_at)
        .await
        .map_err(|e| CreateTokenError::Internal(crate::internal!(e)))?;

    Ok(Json(CreateApiTokenResponse {
        token: token_to_dto(model),
        plaintext,
    }))
}

/// Delete (revoke) an API token by ID.
#[salvo::oapi::endpoint(tags("Auth"), summary = "Delete API token")]
pub async fn delete_token(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, DeleteTokenError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = AuthService::new(state.db, state.config.auth, state.config.oidc);
    let deleted = svc
        .delete_api_token(&id, &user.id)
        .await
        .map_err(|e| DeleteTokenError::Internal(crate::internal!(e)))?;
    if !deleted {
        return Err(DeleteTokenError::NotFound);
    }
    Ok(Json(()))
}
