CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS images (
                                      msg_id TEXT NOT NULL,
                                      chat_id TEXT NOT NULL,
                                      vector vector(1024) NOT NULL,
                                      posted_at TIMESTAMPTZ NOT NULL,
                                      PRIMARY KEY (msg_id, chat_id),
                                      CONSTRAINT vector_dims CHECK (array_length(vector::float4[], 1) = 1024)
);

CREATE INDEX IF NOT EXISTS images_vector_idx ON images
    USING ivfflat (vector vector_cosine_ops)
    WITH (lists = 100);

CREATE INDEX IF NOT EXISTS images_posted_at_idx ON images(posted_at);
