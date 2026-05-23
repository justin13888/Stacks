-- tools/seed.sql — sample development data.
--
-- Usage:   just dev-seed <user_id>
-- Apply manually:
--   psql "$WW_DATABASE_URL" -v user_id=01HEXAMPLEULIDHERE -f tools/seed.sql
--
-- The user must already exist (created via the OIDC login flow on first
-- sign-in). This script only adds non-user resources: a smart list, a manual
-- list with two entries, and a few tags. Re-runnable: each insert uses
-- ON CONFLICT DO NOTHING.

\set ON_ERROR_STOP on

\if :{?user_id}
\else
\echo '! Pass -v user_id=<your-user-id> when running this script.'
\quit 1
\endif

-- A manual list with two well-known films.
INSERT INTO user_lists (id, user_id, name, description, is_smart, filter_query,
                        sort_order, created_at, updated_at)
VALUES ('01HSEEDLIST00000000000000A1', :'user_id', 'Favourites',
        'A starter list', false, NULL, NULL, NOW(), NOW())
ON CONFLICT (id) DO NOTHING;

INSERT INTO user_list_items (list_id, cameo_media_id, position, added_at, notes)
VALUES ('01HSEEDLIST00000000000000A1', 'tmdb:movie:550', 1024.0, NOW(),
        'Fight Club (1999)')
ON CONFLICT (list_id, cameo_media_id) DO NOTHING;

INSERT INTO user_list_items (list_id, cameo_media_id, position, added_at, notes)
VALUES ('01HSEEDLIST00000000000000A1', 'tmdb:tv:1396', 2048.0, NOW(),
        'Breaking Bad')
ON CONFLICT (list_id, cameo_media_id) DO NOTHING;

-- A smart list: top-rated movies, popular feed.
INSERT INTO user_lists (id, user_id, name, description, is_smart, filter_query,
                        sort_order, created_at, updated_at)
VALUES ('01HSEEDLIST00000000000000B2', :'user_id', 'Highly rated',
        'Smart list: top-rated movies',
        true,
        '{"media_type": ["movie"], "sort": "top_rated", "score_min": 8}'::jsonb,
        NULL, NOW(), NOW())
ON CONFLICT (id) DO NOTHING;

-- A couple of tags.
INSERT INTO user_tags (id, user_id, name, color, created_at)
VALUES ('01HSEEDTAG0000000000000001', :'user_id', 'rewatch', '#FACC15', NOW()),
       ('01HSEEDTAG0000000000000002', :'user_id', 'comfort',  '#60A5FA', NOW())
ON CONFLICT (id) DO NOTHING;
