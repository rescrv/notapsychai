CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    default_timezone TEXT NOT NULL DEFAULT 'UTC',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);

CREATE TABLE rhythms (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rhythm_type TEXT NOT NULL,
    rhythm_data JSONB NOT NULL,
    description TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    created_at_tz TEXT NOT NULL,
    modified_at TIMESTAMPTZ NOT NULL,
    modified_at_tz TEXT NOT NULL
);

CREATE INDEX idx_rhythms_user_id ON rhythms(user_id);

CREATE TABLE events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rhythm_id UUID NOT NULL REFERENCES rhythms(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    when_utc TIMESTAMPTZ NOT NULL,
    when_tz TEXT NOT NULL
);

CREATE INDEX idx_events_user_id ON events(user_id);
CREATE INDEX idx_events_rhythm_id ON events(rhythm_id);
CREATE INDEX idx_events_when_utc ON events(when_utc);

CREATE TABLE spoons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    date DATE NOT NULL,
    value SMALLINT NOT NULL CHECK (value >= 0 AND value <= 10),
    UNIQUE(user_id, date)
);

CREATE INDEX idx_spoons_user_id_date ON spoons(user_id, date);

CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_sessions_token_hash ON sessions(token_hash);
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);
