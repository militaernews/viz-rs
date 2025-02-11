CREATE EXTENSION vector;
CREATE TABLE images (
                        id UUID PRIMARY KEY,
                        url TEXT NOT NULL,
                        embedding VECTOR(512) NOT NULL
);