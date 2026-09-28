create table media_items (
    id              bigserial primary key,
    dhash           bigint not null,
    embedding_id    uuid not null unique,
    thumb_key       text not null,
    caption         text,
    ocr_text        text,
    media_kind      text not null check (media_kind in ('photo', 'video_frame')),
    frame_offset_ms bigint,
    created_at      timestamptz not null default now(),
    search_vec      tsvector generated always as (
                        to_tsvector('simple', coalesce(caption, '') || ' ' || coalesce(ocr_text, ''))
                    ) stored,
    -- NULLS NOT DISTINCT (Postgres 15+): photos have a null offset, and a plain
    -- unique constraint would never consider two null offsets a collision.
    constraint media_items_dedup_key unique nulls not distinct (dhash, frame_offset_ms)
);

create index media_items_search_idx on media_items using gin (search_vec);
create index media_items_dhash_idx on media_items (dhash);

create table media_sightings (
    id                bigserial primary key,
    media_item_id     bigint not null references media_items(id) on delete cascade,
    channel_id        bigint not null,
    channel_username  text,
    message_id        bigint not null,
    is_backup_channel boolean not null default false,
    posted_at         timestamptz not null,
    -- one video message yields several frames, i.e. several media items
    unique (channel_id, message_id, media_item_id)
);

create index media_sightings_item_idx on media_sightings (media_item_id);

create table watched_channels (
    channel_id       bigint primary key,
    channel_username text,
    is_backup        boolean not null default false,
    is_private       boolean not null default false,
    enabled          boolean not null default true,
    last_message_id  bigint not null default 0
);

create table failed_items (
    channel_id     bigint not null,
    message_id     bigint not null,
    error          text not null,
    retry_count    integer not null default 1,
    last_failed_at timestamptz not null default now(),
    primary key (channel_id, message_id)
);
