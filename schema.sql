-- Follow https://github.com/pgvector/pgvector, requires su
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS images (
                                      id UUID PRIMARY KEY,
                                      msg_id TEXT NOT NULL,
                                      chat_id TEXT NOT NULL,
                                      vector vector(1024) NOT NULL,
                                      timestamp TIMESTAMPTZ NOT NULL,
                                      CONSTRAINT vector_dims CHECK (array_length(vector::float4[], 1) = 1024)
);

CREATE INDEX IF NOT EXISTS images_vector_idx ON images
    USING ivfflat (vector vector_cosine_ops)
    WITH (lists = 100);

CREATE INDEX IF NOT EXISTS images_chat_id_idx ON images(chat_id);
CREATE INDEX IF NOT EXISTS images_timestamp_idx ON images(timestamp);