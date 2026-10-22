create extension if not exists pgcrypto;

create table if not exists regions (
    id uuid primary key default gen_random_uuid(),
    code text not null unique,
    name text not null,
    country text,
    status text not null default 'active' check (status in ('active', 'disabled')),
    created_at timestamptz not null default now()
);

create table if not exists nodes (
    id uuid primary key default gen_random_uuid(),
    region_id uuid not null references regions(id) on delete restrict,
    name text not null unique,
    endpoint text not null,
    server_name text not null,
    transport text not null default 'https-connect' check (transport in ('https-connect')),
    weight integer not null default 1 check (weight >= 1),
    capacity integer not null default 0 check (capacity >= 0),
    target_acl jsonb not null default '[]'::jsonb,
    status text not null default 'active' check (status in ('active', 'disabled', 'draining')),
    cert_fingerprint text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists lines (
    id uuid primary key default gen_random_uuid(),
    code text not null unique,
    name text not null,
    region_id uuid references regions(id) on delete set null,
    policy text not null default 'least-connections' check (policy in ('least-connections', 'round-robin')),
    status text not null default 'active' check (status in ('active', 'disabled')),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists line_nodes (
    line_id uuid not null references lines(id) on delete cascade,
    node_id uuid not null references nodes(id) on delete cascade,
    weight integer not null default 1 check (weight >= 1),
    priority integer not null default 100,
    primary key (line_id, node_id)
);

create table if not exists node_health (
    node_id uuid primary key references nodes(id) on delete cascade,
    healthy boolean not null,
    checked_at timestamptz not null default now(),
    checked_by text,
    latency_ms integer,
    detail text
);

create table if not exists audit_events (
    id bigserial primary key,
    actor text not null,
    action text not null,
    object text not null,
    detail jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now()
);

create table if not exists admin_sessions (
    token_hash text primary key,
    created_at timestamptz not null default now(),
    expires_at timestamptz not null
);

create index if not exists admin_sessions_expires on admin_sessions (expires_at);
