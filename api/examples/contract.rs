//! The OpenAPI 3.1 contract harness (ADR-0002).
//!
//! A small kynos router, independent of the Stacks domain, that exercises
//! every shape the real API will put in its description, so the TypeScript
//! side (`web/contract/`) can prove the orval client generated from it is
//! correct and strictly typed:
//!
//! - nullable fields (`Option<T>`) beside required ones: `Title`;
//! - RFC 9457 problem bodies: `TitleNotFound`, `UploadError`, `DownloadError`;
//! - an internally tagged enum: `Progress`;
//! - one generic `Page<T>` at two instantiations: `Page<Title>`, `Page<Tag>`;
//! - a multipart upload with a file part and a text field: `PosterUpload`;
//! - optional query parameters: `ListTitles`;
//! - a `$ref` with a sibling `description`: `Title::poster`;
//! - a binary download: `download_poster`;
//! - a cookie-authenticated and a bearer-authenticated operation.
//!
//! ```text
//! cargo run -p stacks-api --example contract -- spec            # 3.1 document to stdout
//! cargo run -p stacks-api --example contract -- serve [ADDR]    # serve (default 127.0.0.1:0)
//! ```
//!
//! `serve` prints `listening on http://ADDR` once it is bound, which is what
//! the Vitest round trip waits for. The TestClient conformance suite is the
//! `tests` module at the bottom; `cargo test` runs it.

use std::{
    collections::HashMap,
    env,
    io::Write as _,
    net::{Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
};

use kynos::{
    error::rejection::AuthRejection,
    extract::body::{
        binary::Binary,
        multipart::{FilePart, MultipartForm},
    },
    http::media::Png,
    openapi::{Info, SpecVersion},
    prelude::*,
    response::disposition::ContentDisposition,
    security::{
        Authenticates, Authenticator, Unscoped,
        carrier::{ApiKey, BearerToken},
    },
    server::Server,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The session cookie the harness accepts.
const SESSION: &str = "session-ada";
/// The bearer token the harness accepts.
const TOKEN: &str = "stk_contract";
/// A session and a token belonging to a suspended account.
const SUSPENDED_SESSION: &str = "session-suspended";
const SUSPENDED_TOKEN: &str = "stk_suspended";

// --- Schemas ---

/// What kind of title this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize, kynos::ParamValue)]
#[serde(rename_all = "snake_case")]
enum TitleKind {
    Movie,
    Show,
}

/// A poster image's metadata.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
struct Poster {
    /// Where the image bytes are served.
    url: String,
    /// The caption the uploader gave.
    caption: String,
    /// The image's size in bytes.
    bytes: u64,
}

/// A movie or a show.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
struct Title {
    id: Uuid,
    /// Whether this is a movie or a show.
    kind: TitleKind,
    name: String,
    /// The original-language name; `null` when it matches `name`.
    original_name: Option<String>,
    /// The release year; `null` when unknown.
    year: Option<i32>,
    /// The title's poster, when one was uploaded.
    poster: Option<Poster>,
    /// When the title entered the catalogue.
    added_at: chrono::DateTime<chrono::Utc>,
}

/// A user's tag.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
struct Tag {
    id: Uuid,
    label: String,
}

/// One page of a list.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
struct Page<T> {
    items: Vec<T>,
    /// The cursor for the next page; `null` on the last page.
    next_cursor: Option<String>,
}

/// Where a user is with a title.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Progress {
    /// On the list, not started.
    Planned,
    /// Part way through.
    Watching {
        /// The last episode watched.
        episode: u32,
    },
    /// Finished.
    Completed {
        /// The user's score out of 10, if they gave one.
        score: Option<u8>,
    },
}

/// Who is calling.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
struct Caller {
    subject: String,
    /// How the caller authenticated.
    via: String,
}

// --- Parameters ---

/// What `/titles/{id}…` captures.
#[derive(Schema, PathParams)]
struct TitlePath {
    id: Uuid,
}

/// How the title list is filtered and paged.
#[derive(Schema, QueryParams)]
struct ListTitles {
    /// Resume after this cursor.
    cursor: Option<String>,
    /// At most this many items.
    #[schema(minimum = 1, maximum = 200)]
    limit: Option<u32>,
    /// Only titles of this kind.
    kind: Option<TitleKind>,
}

// --- Bodies ---

/// A poster upload: one image and its caption.
#[derive(Schema, MultipartForm)]
struct PosterUpload {
    /// The caption to show with the poster.
    #[schema(min_length = 1, max_length = 200)]
    caption: String,
    /// The image itself.
    file: FilePart,
}

// --- Errors ---

// One error type per operation shape, so each operation declares exactly the
// statuses it can answer with: `assert_declared_responses_covered` would
// otherwise find a 422 on `getTitle` that only `uploadPoster` can produce.

/// No title has the requested id. Shared by every operation on a title.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("no title with id {0}")]
#[problem(
    base = "https://stacks.example/problems/",
    status = 404,
    title = "Title not found"
)]
struct TitleNotFound(Uuid);

/// What uploading a poster can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = "https://stacks.example/problems/")]
enum UploadError {
    #[error(transparent)]
    #[problem(transparent)]
    TitleNotFound(#[from] TitleNotFound),

    #[error("the upload is not a PNG image")]
    #[problem(status = 422, title = "Unsupported image")]
    NotPng {
        /// The media type the part declared.
        #[problem(extension)]
        content_type: String,
    },
}

/// What downloading a poster can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = "https://stacks.example/problems/")]
enum DownloadError {
    #[error(transparent)]
    #[problem(transparent)]
    TitleNotFound(#[from] TitleNotFound),

    #[error("the title has no poster")]
    #[problem(status = 404, title = "Poster not found")]
    PosterNotFound,
}

// --- Security ---

/// A browser session, in a cookie.
#[derive(SecurityScheme)]
#[security(api_key(in = "cookie", name = "stacks_session"))]
#[security(credential = Caller, description = "The web app's session")]
struct SessionCookie;

/// A script's API token, as a bearer credential.
#[derive(SecurityScheme)]
#[security(bearer)]
#[security(credential = Caller, description = "A personal API token")]
struct ApiToken;

/// Verifies both schemes against fixed values.
///
/// A suspended account's credential is genuine but refused, which is the 403
/// every guarded operation declares. kynos calls `authorize` only when a guard
/// demands scopes, so with none demanded the refusal is `authenticate`'s.
///
/// The values are fixed, so nothing here can be unavailable. A real store can
/// be, and kynos 0.4 has no 503 for that yet: getkono/kynos#993.
struct Verifier;

impl Verifier {
    fn caller(presented: &str, via: &str) -> Result<Caller, AuthRejection> {
        match presented {
            SESSION | TOKEN => Ok(Caller {
                subject: "ada".to_owned(),
                via: via.to_owned(),
            }),
            SUSPENDED_SESSION | SUSPENDED_TOKEN => Err(AuthRejection::forbidden()),
            _ => Err(AuthRejection::unauthenticated()),
        }
    }
}

impl<C: Sync> Authenticator<SessionCookie, C> for Verifier {
    async fn authenticate(&self, presented: ApiKey, _context: &C) -> Result<Caller, AuthRejection> {
        Self::caller(presented.as_str(), "session")
    }

    /// Never reached: `Auth<SessionCookie>` demands no scopes.
    async fn authorize(
        &self,
        _credential: &Caller,
        _scopes: &[Unscoped],
        _context: &C,
    ) -> Result<(), AuthRejection> {
        Ok(())
    }
}

impl<C: Sync> Authenticator<ApiToken, C> for Verifier {
    async fn authenticate(
        &self,
        presented: BearerToken,
        _context: &C,
    ) -> Result<Caller, AuthRejection> {
        Self::caller(presented.as_str(), "token")
    }

    /// Never reached: `Auth<ApiToken>` demands no scopes.
    async fn authorize(
        &self,
        _credential: &Caller,
        _scopes: &[Unscoped],
        _context: &C,
    ) -> Result<(), AuthRejection> {
        Ok(())
    }
}

// --- State ---

/// The harness's in-memory catalogue.
#[derive(Clone)]
struct Store {
    titles: Arc<Vec<Title>>,
    tags: Arc<Vec<Tag>>,
    posters: Arc<Mutex<HashMap<Uuid, (Poster, bytes::Bytes)>>>,
}

impl Store {
    fn seeded() -> Self {
        let added_at = chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::seconds(1_700_000_000);
        let titles = vec![
            Title {
                id: Uuid::from_u128(1),
                kind: TitleKind::Movie,
                name: "Spirited Away".to_owned(),
                original_name: Some("千と千尋の神隠し".to_owned()),
                year: Some(2001),
                poster: None,
                added_at,
            },
            Title {
                id: Uuid::from_u128(2),
                kind: TitleKind::Show,
                name: "Reply 1988".to_owned(),
                original_name: Some("응답하라 1988".to_owned()),
                year: Some(2015),
                poster: None,
                added_at,
            },
            Title {
                id: Uuid::from_u128(3),
                kind: TitleKind::Movie,
                name: "Untitled Project".to_owned(),
                original_name: None,
                year: None,
                poster: None,
                added_at,
            },
        ];
        let tags = vec![
            Tag {
                id: Uuid::from_u128(10),
                label: "comfort".to_owned(),
            },
            Tag {
                id: Uuid::from_u128(11),
                label: "rewatch".to_owned(),
            },
        ];
        Self {
            titles: Arc::new(titles),
            tags: Arc::new(tags),
            posters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn title(&self, id: Uuid) -> Result<Title, TitleNotFound> {
        let mut title = self
            .titles
            .iter()
            .find(|title| title.id == id)
            .cloned()
            .ok_or(TitleNotFound(id))?;
        title.poster = self.posters().get(&id).map(|(poster, _)| poster.clone());
        Ok(title)
    }

    fn posters(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, (Poster, bytes::Bytes)>> {
        // A poisoned lock only means another request panicked mid-insert; the
        // map itself is still a valid map.
        self.posters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The application context.
#[derive(Provider)]
struct App {
    store: Store,
    #[provide(skip)]
    verifier: Verifier,
}

impl Authenticates<SessionCookie> for App {
    type Authenticator = Verifier;

    fn authenticator(&self) -> &Verifier {
        &self.verifier
    }
}

impl Authenticates<ApiToken> for App {
    type Authenticator = Verifier;

    fn authenticator(&self) -> &Verifier {
        &self.verifier
    }
}

// --- Operations ---

/// Lists titles.
///
/// The cursor is the index of the next item, which is enough for a harness.
#[kynos::get("/titles", operation_id = "listTitles")]
async fn list_titles(
    Query(query): Query<ListTitles>,
    Inject(store): Inject<Store>,
) -> Json<Page<Title>> {
    let start = query
        .cursor
        .as_deref()
        .and_then(|cursor| cursor.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = query.limit.map_or(50, |limit| limit as usize);
    let matching: Vec<Title> = store
        .titles
        .iter()
        .filter(|title| query.kind.is_none_or(|kind| title.kind == kind))
        .cloned()
        .collect();
    let end = start.saturating_add(limit).min(matching.len());
    let items = matching.get(start..end).unwrap_or_default().to_vec();
    let next_cursor = (end < matching.len()).then(|| end.to_string());
    Json(Page { items, next_cursor })
}

/// Fetches one title.
#[kynos::get("/titles/{id}", operation_id = "getTitle")]
async fn get_title(
    Path(path): Path<TitlePath>,
    Inject(store): Inject<Store>,
) -> Result<Json<Title>, TitleNotFound> {
    store.title(path.id).map(Json)
}

/// Reads the caller's progress on a title.
#[kynos::get("/titles/{id}/progress", operation_id = "getProgress")]
async fn get_progress(
    Path(path): Path<TitlePath>,
    Inject(store): Inject<Store>,
) -> Result<Json<Progress>, TitleNotFound> {
    let title = store.title(path.id)?;
    let progress = match title.id.as_u128() {
        1 => Progress::Completed { score: Some(10) },
        2 => Progress::Watching { episode: 7 },
        _ => Progress::Planned,
    };
    Ok(Json(progress))
}

/// Uploads a title's poster.
#[kynos::post("/titles/{id}/poster", operation_id = "uploadPoster")]
async fn upload_poster(
    Path(path): Path<TitlePath>,
    Inject(store): Inject<Store>,
    MultipartForm(upload): MultipartForm<PosterUpload>,
) -> Result<Created<Json<Poster>>, UploadError> {
    store.title(path.id)?;
    let content_type = upload.file.content_type.unwrap_or_default();
    if content_type != "image/png" {
        return Err(UploadError::NotPng { content_type });
    }
    let location = download_poster::relative_uri(TitlePath { id: path.id });
    let poster = Poster {
        url: location.to_string(),
        caption: upload.caption,
        bytes: upload.file.bytes.len() as u64,
    };
    store
        .posters()
        .insert(path.id, (poster.clone(), upload.file.bytes));
    Ok(Created::at(location, Json(poster)))
}

/// Downloads a title's poster.
#[kynos::get("/titles/{id}/poster", operation_id = "downloadPoster")]
async fn download_poster(
    Path(path): Path<TitlePath>,
    Inject(store): Inject<Store>,
) -> Result<WithHeaders<Binary<Png>, ContentDisposition>, DownloadError> {
    store.title(path.id)?;
    let bytes = store
        .posters()
        .get(&path.id)
        .map(|(_, bytes)| bytes.clone())
        .ok_or(DownloadError::PosterNotFound)?;
    Ok(WithHeaders::new(
        Binary::new(bytes),
        ContentDisposition::attachment().filename(format!("{}.png", path.id)),
    ))
}

/// Lists the caller's tags.
#[kynos::get("/tags", operation_id = "listTags")]
async fn list_tags(Inject(store): Inject<Store>) -> Json<Page<Tag>> {
    Json(Page {
        items: store.tags.to_vec(),
        next_cursor: None,
    })
}

/// Reads the session's caller (cookie authentication).
#[kynos::get("/session", operation_id = "getSession")]
async fn get_session(auth: Auth<SessionCookie>) -> Json<Caller> {
    Json(auth.into_inner())
}

/// Reads the token's caller (bearer authentication).
#[kynos::get("/me", operation_id = "getMe")]
async fn get_me(auth: Auth<ApiToken>) -> Json<Caller> {
    Json(auth.into_inner())
}

// --- Assembly ---

fn router() -> Router<App> {
    Router::<App>::new()
        .info(Info::new(
            "Stacks contract harness",
            env!("CARGO_PKG_VERSION"),
        ))
        .mount(kynos::routes![
            list_titles,
            get_title,
            get_progress,
            upload_poster,
            download_poster,
            list_tags,
            get_session,
            get_me,
        ])
}

fn context() -> App {
    App {
        store: Store::seeded(),
        verifier: Verifier,
    }
}

/// The 3.1 document. Fails, rather than downgrading, if anything needs 3.2.
fn spec() -> kynos::Result<String> {
    Ok(router().openapi_as(SpecVersion::V3_1)?.to_json()?)
}

#[tokio::main]
async fn main() -> kynos::Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("spec") => {
            println!("{}", spec()?);
            Ok(())
        }
        Some("serve") => {
            let addr: SocketAddr = match args.next() {
                Some(raw) => raw.parse().unwrap_or_else(|error| {
                    eprintln!("invalid address {raw:?}: {error}");
                    std::process::exit(2);
                }),
                None => (Ipv4Addr::LOCALHOST, 0).into(),
            };
            let server = Server::new(router().build(context())?)
                .bind(addr)
                .prepare()
                .await?;
            if let Some(bound) = server.local_addrs().first() {
                println!("listening on http://{bound}");
                // Flushed at once: the round trip reads this line through a pipe.
                let _ = std::io::stdout().flush();
            }
            server.serve().await
        }
        _ => {
            eprintln!("usage: contract (spec | serve [ADDR])");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Every harness operation, driven in-process and checked against the
    //! description it emits.

    use kynos::{http::StatusCode, test::TestClient};

    use super::*;

    const BOUNDARY: &str = "contract-boundary";
    /// The problem `type` prefix `TitleNotFound`, `UploadError` and
    /// `DownloadError` declare.
    const PROBLEM_BASE: &str = "https://stacks.example/problems/";
    /// The 8-byte PNG signature, which is all an uploaded "image" needs here.
    const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

    fn multipart(caption: &str, content_type: &str, bytes: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"caption\"\r\n\r\n{caption}\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"p.png\"\r\nContent-Type: {content_type}\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
        body
    }

    fn multipart_type() -> String {
        format!("multipart/form-data; boundary={BOUNDARY}")
    }

    #[test]
    fn the_document_is_3_1_and_validates() {
        let document: serde_json::Value =
            serde_json::from_str(&spec().expect("3.1 emission")).expect("JSON");
        let version = document["openapi"].as_str().expect("openapi version");
        assert!(version.starts_with("3.1."), "emitted {version}");

        let violations = router().validate().expect("validation runs");
        assert!(violations.is_empty(), "violations: {violations:?}");
    }

    /// A 3.2-only construct makes the 3.1 export fail instead of being dropped.
    ///
    /// With `openapi32` off a router cannot declare one (the constructs do not
    /// compile), so the harness's own document gains a 3.2 `QUERY` operation
    /// and goes through the same `emit` step `openapi_as` ends in.
    #[test]
    fn a_3_2_only_construct_fails_the_3_1_export() {
        use kynos::openapi::{Document, SpecError};

        let emitted = router()
            .openapi_as(SpecVersion::V3_1)
            .expect("3.1 emission");
        let mut value = serde_json::to_value(&emitted).expect("JSON");
        value["paths"]["/tags"]["query"] =
            serde_json::json!({ "responses": { "200": { "description": "ok" } } });
        let widened: Document = serde_json::from_value(value).expect("document parses");

        match widened.emit(SpecVersion::V3_1) {
            Err(SpecError::RequiresV3_2 { blockers }) => {
                assert_eq!(blockers, ["#/paths/~1tags/query"]);
            }
            other => panic!("expected a 3.2 refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn every_operation_conforms_and_every_declared_response_is_exercised() {
        let client = TestClient::new(router().build(context()).expect("router builds"));
        let missing = Uuid::from_u128(99);

        // listTitles: defaults, every optional parameter, and a bad one.
        let page: Page<Title> = client
            .get("/titles")
            .send()
            .await
            .assert_status(StatusCode::OK)
            .json();
        assert_eq!(page.items.len(), 3);
        assert_eq!(page.next_cursor, None);
        let page: Page<Title> = client
            .get("/titles?limit=1&cursor=1&kind=movie")
            .send()
            .await
            .assert_status(StatusCode::OK)
            .json();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items.first().map(|t| t.id), Some(Uuid::from_u128(3)));
        assert_eq!(page.next_cursor, None);
        // A cursor past every item is an empty last page, not an overflow.
        let page: Page<Title> = client
            .get(&format!("/titles?cursor={}", usize::MAX))
            .send()
            .await
            .assert_status(StatusCode::OK)
            .json();
        assert!(page.items.is_empty());
        assert_eq!(page.next_cursor, None);
        client
            .get("/titles?limit=0")
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);

        // getTitle: found, not found, and an id that is not a UUID.
        let title: Title = client
            .get(&format!("/titles/{}", Uuid::from_u128(3)))
            .send()
            .await
            .assert_status(StatusCode::OK)
            .json();
        assert_eq!(title.year, None);
        client
            .get(&format!("/titles/{missing}"))
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&format!("{PROBLEM_BASE}title-not-found"));
        client
            .get("/titles/not-a-uuid")
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);

        // getProgress: each variant of the tagged union, and not found.
        for (id, expected) in [
            (1, Progress::Completed { score: Some(10) }),
            (2, Progress::Watching { episode: 7 }),
            (3, Progress::Planned),
        ] {
            let progress: Progress = client
                .get(&format!("/titles/{}/progress", Uuid::from_u128(id)))
                .send()
                .await
                .assert_status(StatusCode::OK)
                .json();
            assert_eq!(progress, expected);
        }
        client
            .get(&format!("/titles/{missing}/progress"))
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND);
        client
            .get("/titles/not-a-uuid/progress")
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);

        // downloadPoster: an unknown title, a bad id, and no poster yet.
        client
            .get(&format!("/titles/{missing}/poster"))
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&format!("{PROBLEM_BASE}title-not-found"));
        client
            .get("/titles/not-a-uuid/poster")
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);
        let first = Uuid::from_u128(1);
        client
            .get(&format!("/titles/{first}/poster"))
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&format!("{PROBLEM_BASE}poster-not-found"));

        // uploadPoster: success, the wrong media type, and a missing part.
        let poster: Poster = client
            .post(&format!("/titles/{first}/poster"))
            .body(
                &multipart_type(),
                multipart("Key art", "image/png", PNG_SIGNATURE),
            )
            .send()
            .await
            .assert_status(StatusCode::CREATED)
            .json();
        assert_eq!(poster.bytes, PNG_SIGNATURE.len() as u64);
        client
            .post(&format!("/titles/{first}/poster"))
            .body(
                &multipart_type(),
                multipart("Key art", "image/gif", b"GIF89a"),
            )
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&format!("{PROBLEM_BASE}not-png"));
        client
            .post(&format!("/titles/{first}/poster"))
            .text("not multipart")
            .send()
            .await
            .assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        // A body that is not multipart at all, though it says it is.
        client
            .post(&format!("/titles/{first}/poster"))
            .body(&multipart_type(), "garbage")
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);
        // Well-formed multipart missing the `file` part.
        let caption_only = format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"caption\"\r\n\r\nKey art\r\n--{BOUNDARY}--\r\n"
        );
        client
            .post(&format!("/titles/{first}/poster"))
            .body(&multipart_type(), caption_only)
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type("about:blank");
        // A declared length past the 2 MiB default body limit.
        client
            .post(&format!("/titles/{first}/poster"))
            .body(
                &multipart_type(),
                multipart("Key art", "image/png", PNG_SIGNATURE),
            )
            .content_length(3 * 1024 * 1024)
            .send()
            .await
            .assert_status(StatusCode::PAYLOAD_TOO_LARGE);
        client
            .post(&format!("/titles/{missing}/poster"))
            .body(
                &multipart_type(),
                multipart("Key art", "image/png", PNG_SIGNATURE),
            )
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&format!("{PROBLEM_BASE}title-not-found"));
        client
            .post("/titles/not-a-uuid/poster")
            .body(
                &multipart_type(),
                multipart("Key art", "image/png", PNG_SIGNATURE),
            )
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);

        // downloadPoster after the upload: the same bytes come back.
        let download = client.get(&format!("/titles/{first}/poster")).send().await;
        download.assert_status(StatusCode::OK);
        assert_eq!(download.bytes().as_ref(), PNG_SIGNATURE);

        // The upload is now visible through getTitle's `$ref` property.
        let title: Title = client
            .get(&format!("/titles/{first}"))
            .send()
            .await
            .assert_status(StatusCode::OK)
            .json();
        assert_eq!(title.poster, Some(poster));

        // listTags: the second Page<T> instantiation.
        let tags: Page<Tag> = client
            .get("/tags")
            .send()
            .await
            .assert_status(StatusCode::OK)
            .json();
        assert_eq!(tags.items.len(), 2);

        // getSession and getMe: accepted and refused.
        let caller: Caller = client
            .get("/session")
            .cookie("stacks_session", SESSION)
            .send()
            .await
            .assert_status(StatusCode::OK)
            .json();
        assert_eq!(caller.via, "session");
        client
            .get("/session")
            .send()
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
        let caller: Caller = client
            .get("/me")
            .header("authorization", &format!("Bearer {TOKEN}"))
            .send()
            .await
            .assert_status(StatusCode::OK)
            .json();
        assert_eq!(caller.via, "token");
        client
            .get("/me")
            .header("authorization", "Bearer wrong")
            .send()
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
        // Authenticated but suspended: the 403 both guards declare.
        client
            .get("/session")
            .cookie("stacks_session", SUSPENDED_SESSION)
            .send()
            .await
            .assert_status(StatusCode::FORBIDDEN);
        client
            .get("/me")
            .header("authorization", &format!("Bearer {SUSPENDED_TOKEN}"))
            .send()
            .await
            .assert_status(StatusCode::FORBIDDEN);

        client.assert_conformance();
        client.assert_declared_responses_covered();
    }
}
