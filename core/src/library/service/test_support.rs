//! Library shorthands only tests use (the `test-support` feature): one play
//! at a time, and unwatching an episode by number.

use super::*;

/// What to record for one play.
#[derive(Debug, Clone)]
pub struct LogPlay {
    /// `(season, episode)` for a show; `None` for a movie.
    pub episode: Option<(i32, i32)>,
    /// When it was watched, if known.
    pub watched_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
    pub source: PlaySource,
    /// The import a play came from (only with [`PlaySource::Import`]).
    pub import_id: Option<Uuid>,
}

impl LogPlay {
    /// A manual play watched now.
    pub fn manual(episode: Option<(i32, i32)>) -> Self {
        Self {
            episode,
            watched_at: Some(crate::db::now()),
            source: PlaySource::Manual,
            import_id: None,
        }
    }
}

impl LibraryService {
    /// Record a play of a movie or an episode: [`log_plays`](Self::log_plays)
    /// with a batch of one.
    pub async fn log_play(
        &self,
        user_id: Uuid,
        media: &media::Model,
        log: LogPlay,
    ) -> Result<play::Model, LibraryError> {
        let batch = match log.episode {
            Some(episode) => PlayBatch::Episodes(vec![episode]),
            None => PlayBatch::Movie,
        };
        let mut logged = self
            .log_plays(
                user_id,
                media,
                batch,
                log.watched_at,
                log.source,
                log.import_id,
            )
            .await?;
        // A batch of one play that skips nothing logs exactly one.
        logged.pop().ok_or(LibraryError::PlayNotFound)
    }

    /// Mark an episode unwatched in the current run by voiding its latest
    /// live play. Returns the voided play, or `None` if it was not watched.
    pub async fn unwatch_episode(
        &self,
        user_id: Uuid,
        media_id: Uuid,
        season: i32,
        episode: i32,
    ) -> Result<Option<play::Model>, LibraryError> {
        match self
            .episode_play(user_id, media_id, season, episode)
            .await?
        {
            Some(p) => self.void_play(user_id, p.id).await.map(Some),
            None => Ok(None),
        }
    }
}
